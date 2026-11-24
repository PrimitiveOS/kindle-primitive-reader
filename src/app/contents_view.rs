//! The Contents screen projects provider headings without changing reading targets.
use primitive_ui::{BLACK, HAIRLINE, Icon, PAPER, SECONDARY, TextStyle};

use super::contents_position::ContentsPositions;
use super::contents_tree::{Branch, ContentsTree, SearchOutcome, VisibleRow};
use super::list_density::{Geometry, list_text_pixels, nominal_height, text_line_height};
use super::{
    Action, CAPTION, Canvas, Change, CustomAction, DocumentFormat, Editor, HEIGHT, Input, Item,
    LIST_X, Press, Reader, Rect, Screen, keyboard_key,
};
use crate::epub::Destination;
use crate::generated_contents::OutlineTarget;

mod row_metadata;
#[cfg(test)]
mod row_metadata_tests;

const SEARCH: Rect = Rect::new(338, HEIGHT as i32 - 104, 276, 72);
const CLEAR: Rect = Rect::new(653, HEIGHT as i32 - 104, 276, 72);
const QUERY_LIMIT: usize = 128;
const EXPAND: Icon = Icon {
    stroke: &["M9 5 L16 12 L9 19"],
    fill: &[],
};
const COLLAPSE: Icon = Icon {
    stroke: &["M5 9 L12 16 L19 9"],
    fill: &[],
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Target {
    Reflow(Destination),
    Alternative(OutlineTarget),
    Fixed(usize),
    Custom(usize),
    None,
}

#[derive(Eq, PartialEq)]
pub(super) struct ContentsEntry {
    title: String,
    pub(super) depth: usize,
    pub(super) target: Target,
}

pub(super) struct ContentsView {
    book: u64,
    entries: Vec<ContentsEntry>,
    tree: ContentsTree,
    hierarchical: bool,
    positions: Option<ContentsPositions>,
    previous_page: usize,
}

struct ContentsLine {
    item: Item,
    heading: Option<VisibleRow>,
}

impl Screen {
    pub(super) fn is_contents(self) -> bool {
        matches!(self, Self::Contents | Self::FixedContents)
    }
}

impl Reader {
    fn contents_geometry(&self) -> Geometry {
        Geometry::dense(self.controls.contents_density).in_navigation(self.navigation_layout())
    }

    pub(super) fn contents_rows(&self) -> usize {
        self.contents_geometry().rows
    }

    pub(super) fn contents_row_rect(&self, offset: usize) -> Rect {
        self.contents_geometry().row(offset)
    }

    #[cfg(test)]
    pub(super) fn contents_branch_button(&self, offset: usize) -> Option<(Rect, Branch)> {
        let rows = self.contents_rows();
        let (_, lines) = self.contents_window(self.list_page * rows, rows);
        let heading = lines.get(offset)?.heading?;
        Some((
            branch_rect(self.contents_row_rect(offset), heading)?,
            heading.branch,
        ))
    }

    pub(super) fn prepare_contents(&mut self, reveal: bool) {
        let Some(book) = self.open_book_id() else {
            self.contents_view = None;
            return;
        };
        let entries = self.contents_entries(book);
        let positions = self.contents_positions(&entries);
        let view = self
            .contents_view
            .take()
            .filter(|view| view.book == book && view.entries == entries);
        let mut view = view.unwrap_or_else(|| ContentsView {
            book,
            tree: ContentsTree::new(entries.iter().map(|entry| entry.depth)),
            hierarchical: entries.windows(2).any(|pair| pair[1].depth > pair[0].depth),
            entries,
            positions: None,
            previous_page: 0,
        });
        if reveal && let Some(positions) = &positions {
            view.tree.reveal_current(positions.reveal());
        }
        view.positions = positions;
        self.contents_view = Some(view);
        self.update_contents_focus();
    }

    pub(super) fn contents_entries(&self, book: u64) -> Vec<ContentsEntry> {
        if let Some(state) = self
            .library
            .state(book)
            .filter(|state| state.custom_contents.enabled)
        {
            return state
                .custom_contents
                .entries()
                .iter()
                .enumerate()
                .map(|(index, entry)| ContentsEntry {
                    title: entry.label.clone(),
                    depth: 0,
                    target: Target::Custom(index),
                })
                .collect();
        }
        if let Some(fixed) = &self.fixed {
            return fixed
                .reading
                .outline()
                .iter()
                .map(|entry| ContentsEntry {
                    title: entry.title.clone(),
                    depth: entry.depth,
                    target: entry.page.map_or(Target::None, Target::Fixed),
                })
                .collect();
        }
        if let Some(entries) = self.alternative_entries(book) {
            return entries
                .iter()
                .map(|entry| ContentsEntry {
                    title: entry.title.clone(),
                    depth: entry.depth,
                    target: Target::Alternative(entry.target.clone()),
                })
                .collect();
        }
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let probe = &active.reading.probe;
        if probe.toc.is_empty() {
            return probe
                .chapter_entries()
                .iter()
                .map(|chapter| ContentsEntry {
                    title: chapter.label.clone(),
                    depth: 0,
                    target: Target::Reflow(Destination {
                        path: chapter.href.clone(),
                        fragment: None,
                    }),
                })
                .collect();
        }
        probe
            .toc
            .iter()
            .map(|entry| ContentsEntry {
                title: entry.label.clone(),
                depth: entry.depth,
                target: entry
                    .destination
                    .clone()
                    .map_or(Target::None, Target::Reflow),
            })
            .collect()
    }

    fn contents_controls(&self) -> Vec<Item> {
        let mut controls = Vec::new();
        let custom = self.custom_contents_enabled();
        if self.pending_alternative_contents() {
            controls.push(Item::new("Building contents…", Action::Heading));
        } else if self.failed_alternative_contents() {
            controls.push(
                Item::new("Showing publisher contents", Action::Heading)
                    .detail("The alternative contents could not be built"),
            );
        } else if !custom
            && !self.uses_alternative_contents()
            && self
                .active
                .as_ref()
                .is_some_and(|active| active.entry.format == DocumentFormat::Epub)
        {
            controls.push(
                Item::new("Generated outline", Action::ShowGeneratedContents)
                    .detail("Build from headings in the book"),
            );
        }
        controls.push(Item::new(
            if custom {
                "Manage custom contents"
            } else {
                "Custom contents"
            },
            Action::Show(Screen::CustomContents),
        ));
        controls
    }

    fn update_contents_focus(&mut self) {
        self.current_contents_row = self.contents_view.as_ref().and_then(|view| {
            // KOReader highlights the reading location in the full outline and
            // the expanded-all view, but treats title results as a separate list.
            if view.tree.query().is_some_and(|query| query != "*") {
                return None;
            }
            view.positions
                .as_ref()?
                .focus(view.tree.visible_rows().map(|row| row.source_index))
                .map(|row| row + self.contents_controls().len())
        });
    }

    fn contents_window(&self, first: usize, limit: usize) -> (usize, Vec<ContentsLine>) {
        let controls = self.contents_controls();
        let prefix = controls.len();
        let mut lines: Vec<_> = controls
            .into_iter()
            .skip(first)
            .take(limit)
            .map(|item| ContentsLine {
                item,
                heading: None,
            })
            .collect();
        let Some(view) = &self.contents_view else {
            return (prefix, lines);
        };
        let visible = view.tree.visible_rows();
        let count = visible.count() + prefix;
        lines.extend(
            view.tree
                .visible_rows()
                .skip(first.saturating_sub(prefix))
                .take(limit - lines.len())
                .map(|heading| {
                    let entry = &view.entries[heading.source_index];
                    let action = match &entry.target {
                        Target::Reflow(target) => Action::Jump(target.clone()),
                        Target::Alternative(target) => {
                            Action::AlternativeContentsEntry(target.clone())
                        }
                        Target::Fixed(page) => Action::FixedPage(*page),
                        Target::Custom(index) => Action::Custom(CustomAction::Open(*index)),
                        Target::None => Action::Heading,
                    };
                    let mut item = Item::new(&entry.title, action);
                    if let Target::Custom(index) = entry.target
                        && let Some(mark) = self
                            .library
                            .state(view.book)
                            .and_then(|state| state.custom_contents.entries().get(index))
                    {
                        item.detail = Some(if self.fixed.is_some() {
                            format!("Page {}", mark.position.page.saturating_add(1))
                        } else {
                            format!("Chapter {}", mark.position.chapter.saturating_add(1))
                        });
                    }
                    ContentsLine {
                        item,
                        heading: Some(heading),
                    }
                }),
        );
        (count, lines)
    }

    pub(super) fn contents_items(&self, first: usize, limit: usize) -> (usize, Vec<Item>) {
        let (count, lines) = self.contents_window(first, limit);
        (count, lines.into_iter().map(|line| line.item).collect())
    }

    pub(super) fn draw_contents(&self, canvas: &mut Canvas) {
        let rows = self.contents_rows();
        let first = self.list_page * rows;
        let (count, lines) = self.contents_window(first, rows);
        if !lines.is_empty() {
            canvas.card(self.contents_geometry().bounds(lines.len()));
        }
        let row_height = self.contents_row_rect(0).h;
        let font = self.controls.contents_density.font();
        let nominal_row = nominal_height(canvas, row_height);
        let title_size = list_text_pixels(font, nominal_row);
        let title_height = text_line_height(canvas, title_size);
        let detail_size = list_text_pixels(font.saturating_sub(4), nominal_row)
            .min(nominal_height(canvas, row_height - title_height - 4) * 10 / 13)
            .min(28);
        let detail_height = text_line_height(canvas, detail_size);
        let branch_column = self
            .contents_view
            .as_ref()
            .is_some_and(|view| view.hierarchical);
        for (offset, line) in lines.iter().enumerate() {
            let row = self.contents_row_rect(offset);
            let branch = line.heading.and_then(|heading| branch_rect(row, heading));
            let body = heading_body(row, branch);
            let link = !matches!(line.item.action, Action::Heading);
            let pressed = link && canvas.control_pressed(body, false);
            if pressed {
                canvas.panel(body.inset(6), 16, 2, BLACK);
            }
            let ink = if pressed { PAPER } else { BLACK };
            let detail_ink = if pressed {
                PAPER - SECONDARY
            } else {
                SECONDARY
            };
            let title_x = line.heading.map_or(row.x + 32, |heading| {
                row.x + 32 + indent(heading) + i32::from(branch_column) * 86
            });
            let current = self.current_contents_row == Some(first + offset);
            let detail = match (current, &line.item.detail) {
                (true, Some(detail)) => Some(format!("Current section · {detail}")),
                (true, None) => Some("Current section".into()),
                (false, detail) => detail.clone(),
            };
            let detail = detail.filter(|_| detail_size >= 20);
            let text_height = title_height + i32::from(detail.is_some()) * detail_height;
            let title_y = row.y + (row.h - text_height) / 2;
            let width = row.right() - title_x - if link { 86 } else { 32 };
            let numbers = line.heading.and_then(|heading| {
                self.contents_view
                    .as_ref()?
                    .positions
                    .as_ref()?
                    .numbers(heading.source_index)
            });
            self.draw_contents_title(
                canvas,
                Rect::new(title_x, title_y, width, title_height),
                &line.item.title,
                numbers,
                row_metadata::RowTypography {
                    title_size,
                    title_bold: current || line.heading.is_none(),
                    info_size: list_text_pixels(font.saturating_sub(4), nominal_row),
                    ink,
                    info_ink: detail_ink,
                },
            );
            if let Some(detail) = detail {
                canvas.text(
                    Rect::new(title_x, title_y + title_height, width, detail_height),
                    &detail,
                    TextStyle::new(detail_size).ink(detail_ink),
                );
            }
            if let (Some(branch), Some(heading)) = (branch, line.heading) {
                let icon = if heading.branch == Branch::Expanded {
                    COLLAPSE
                } else {
                    EXPAND
                };
                let size = 76.min(branch.h - 10);
                let face = Rect::new(
                    branch.x + (branch.w - size) / 2,
                    branch.y + (branch.h - size) / 2,
                    size,
                    size,
                );
                let pressed = canvas.control_pressed(branch, false);
                canvas.button(face, "", false, pressed);
                let ink = if pressed || canvas.is_control_pressed(face) {
                    PAPER
                } else {
                    BLACK
                };
                canvas.icon(face.inset(size / 4), icon, ink);
            }
            if link {
                primitive_ui::chevron(
                    canvas,
                    Rect::new(row.right() - 72, row.y + (row.h - 40) / 2, 40, 40),
                    ink,
                );
            }
            if offset + 1 < lines.len() {
                canvas.fill(
                    Rect::new(row.x + 32, row.bottom() - 2, row.w - 64, 2),
                    HAIRLINE,
                );
            }
        }

        let navigation = self.navigation_layout();
        if self.list_page > 0 {
            footer_button(canvas, navigation.previous(), "Previous");
        }
        if (self.list_page + 1) * rows < count {
            footer_button(canvas, navigation.next(), "Next");
        }
        let query = self
            .contents_view
            .as_ref()
            .and_then(|view| view.tree.query());
        if self
            .contents_view
            .as_ref()
            .is_some_and(|view| !view.entries.is_empty())
        {
            footer_button(canvas, navigation.centered_footer(SEARCH), "Search");
        }
        if query.is_some() {
            footer_button(canvas, navigation.centered_footer(CLEAR), "Clear");
        }
        let pages = count.div_ceil(rows);
        if pages > 1 {
            canvas.text(
                navigation.footer_text(LIST_X, 230, 46),
                &format!("{} / {pages}", self.list_page + 1),
                CAPTION.center(),
            );
        }
        let info = self
            .notice
            .clone()
            .or_else(|| query.map(|query| format!("Query: {query}")));
        if let Some(info) = info {
            canvas.text(
                navigation.footer_text(LIST_X, 173, 52),
                &info,
                CAPTION.center(),
            );
        }
    }

    pub(super) fn contents_input(&mut self, input: Input) -> Change {
        match input {
            Input::Tap(x, y) => self.contents_tap(x, y),
            Input::PageForward => self.turn_contents_page(1),
            Input::PageBack => self.turn_contents_page(-1),
            Input::Swipe { dy, dx, .. } if dy.abs() > dx.abs() => {
                self.turn_contents_page(if dy < 0.0 { 1 } else { -1 })
            }
            _ => Change::None,
        }
    }

    fn contents_tap(&mut self, x: f32, y: f32) -> Change {
        let navigation = self.navigation_layout();
        if navigation.back().contains(x, y) {
            self.show(self.tools_screen());
            return Change::Partial;
        }
        if footer_hit(navigation.centered_footer(SEARCH)).contains(x, y)
            && let Some(view) = &mut self.contents_view
            && !view.entries.is_empty()
        {
            view.previous_page = self.list_page;
            self.editor = Editor::new(QUERY_LIMIT);
            if let Some(query) = view.tree.query() {
                self.editor.text = query.chars().collect();
                self.editor.caret = self.editor.text.len();
            }
            self.show(Screen::ContentsSearch);
            return Change::Partial;
        }
        if footer_hit(navigation.centered_footer(CLEAR)).contains(x, y)
            && self
                .contents_view
                .as_mut()
                .is_some_and(|view| view.tree.clear_search())
        {
            self.show(self.contents_screen());
            return Change::Partial;
        }
        if footer_hit(navigation.previous()).contains(x, y) {
            return self.turn_contents_page(-1);
        }
        if footer_hit(navigation.next()).contains(x, y) {
            return self.turn_contents_page(1);
        }
        let geometry = self.contents_geometry();
        let Some(offset) = geometry.offset_at(geometry.rows, x, y) else {
            return Change::None;
        };
        let index = self.list_page * geometry.rows + offset;
        let (_, mut lines) = self.contents_window(index, 1);
        let Some(line) = lines.pop() else {
            return Change::None;
        };
        let row = self.contents_row_rect(offset);
        if let Some(heading) = line.heading
            && let Some(branch) = branch_rect(row, heading)
        {
            if branch.contains(x, y) {
                if self
                    .contents_view
                    .as_mut()
                    .is_some_and(|view| view.tree.toggle(heading.source_index))
                {
                    self.update_contents_focus();
                    self.clamp_contents_page();
                    self.notice = None;
                    return Change::Partial;
                }
                return Change::None;
            }
            if !heading_body(row, Some(branch)).contains(x, y) {
                return Change::None;
            }
        }
        match self.activate(line.item.action) {
            Ok(()) => self.applied_change(),
            Err(error) => self.report(error),
        }
    }

    fn turn_contents_page(&mut self, direction: isize) -> Change {
        let rows = self.contents_rows();
        let (count, _) = self.contents_window(0, 0);
        let Some(page) = self
            .list_page
            .checked_add_signed(direction)
            .filter(|page| page * rows < count)
        else {
            return Change::None;
        };
        self.list_page = page;
        Change::Partial
    }

    pub(super) fn contents_search_input(&mut self, input: Input) -> Change {
        let navigation = self.navigation_layout();
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if navigation.back().contains(x, y))
        {
            self.return_from_contents_search(None);
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = navigation.keyboard();
        let Some(key) = keyboard_key(&layout, &self.editor, "Find", x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let query = self.editor.string();
            let result = self.contents_view.as_mut().map(|view| {
                view.tree
                    .search(&query, |index| view.entries[index].title.as_str())
            });
            self.return_from_contents_search(result);
        }
        Change::Partial
    }

    fn return_from_contents_search(&mut self, result: Option<SearchOutcome>) {
        let previous_page = self
            .contents_view
            .as_ref()
            .map_or(0, |view| view.previous_page);
        self.show(self.contents_screen());
        match result {
            Some(SearchOutcome::Applied) => self.list_page = 0,
            _ => {
                self.list_page = previous_page;
                self.clamp_contents_page();
            }
        }
        if result == Some(SearchOutcome::NoMatches) {
            self.notice = Some("No matching headings".into());
        }
    }

    fn clamp_contents_page(&mut self) {
        let (count, _) = self.contents_window(0, 0);
        self.list_page = self
            .list_page
            .min(count.saturating_sub(1) / self.contents_rows());
    }
}

fn indent(heading: VisibleRow) -> i32 {
    heading.depth.min(12) as i32 * 36
}

fn branch_rect(row: Rect, heading: VisibleRow) -> Option<Rect> {
    (heading.branch != Branch::Leaf).then(|| {
        // The title keeps its own hit area even when adjacent rows are dense.
        let height = row.h.min(100);
        Rect::new(
            row.x + 12 + indent(heading),
            row.y + (row.h - height) / 2,
            100,
            height,
        )
    })
}

fn heading_body(row: Rect, branch: Option<Rect>) -> Rect {
    let x = branch.map_or(row.x, |branch| branch.right());
    Rect::new(x, row.y, row.right() - x, row.h)
}

fn footer_hit(face: Rect) -> Rect {
    // Keep the shared paging bar's 72px face with a 100px touch target.
    face.inset(-14)
}

fn footer_button(canvas: &mut Canvas, face: Rect, title: &str) {
    let pressed = canvas.control_pressed(footer_hit(face), false);
    canvas.button(face, title, false, pressed);
}

#[cfg(test)]
mod tests;
