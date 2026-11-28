//! Recognize single-contact direction chains from the SDK's viewport-projected pan stream.
use super::book_shortcuts;
use super::{Action, App, Binding, Change, Gesture, Item, Reader, Screen, read_import_file};
use crate::Result;
use primitive_app::{PanPhase, TouchPan};
use primitive_ui::{BLACK, Canvas, Icon, PAPER, Rect};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

// Thirty-two four-bit direction codes fit in a u128. The KOReader detector has
// no practical leg cap; this bound is far beyond what the Oasis 900 ms gesture
// window and sampled touch stream can produce while keeping storage bounded.
const MAX_LEGS: usize = 32;
const MAX_CUSTOM: usize = 64;
const MAX_ACTIONS: usize = 16;
// KOReader's threshold is ceil(50 × 300 / 160) at the SDK's Oasis reference DPI.
const MIN_LEG: f32 = 94.0;
const SWIPE_WINDOW: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(in crate::app) struct Shape(u128);

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Direction {
    N = 1,
    E,
    S,
    W,
    NE,
    SE,
    SW,
    NW,
}

impl Direction {
    const ALL: [(Self, &'static str, &'static str); 8] = [
        (Self::N, "north", "↑"),
        (Self::E, "east", "→"),
        (Self::S, "south", "↓"),
        (Self::W, "west", "←"),
        (Self::NE, "northeast", "↗"),
        (Self::SE, "southeast", "↘"),
        (Self::SW, "southwest", "↙"),
        (Self::NW, "northwest", "↖"),
    ];

    fn infer(dx: f32, dy: f32, diagonal: bool) -> Self {
        if diagonal {
            return match (dx < 0.0, dy < 0.0) {
                (false, true) => Self::NE,
                (false, false) => Self::SE,
                (true, false) => Self::SW,
                (true, true) => Self::NW,
            };
        }
        if dx.abs() > dy.abs() {
            if dx < 0.0 { Self::W } else { Self::E }
        } else if dy < 0.0 {
            Self::N
        } else {
            Self::S
        }
    }

    fn icon(self) -> Icon {
        let stroke: &'static [&'static str] = match self {
            Self::N => &["M12 20 V4 M7 9 L12 4 L17 9"],
            Self::E => &["M4 12 H20 M15 7 L20 12 L15 17"],
            Self::S => &["M12 4 V20 M7 15 L12 20 L17 15"],
            Self::W => &["M20 12 H4 M9 7 L4 12 L9 17"],
            Self::NE => &["M5 19 L19 5 M12 5 H19 V12"],
            Self::SE => &["M5 5 L19 19 M19 12 V19 H12"],
            Self::SW => &["M19 5 L5 19 M12 19 H5 V12"],
            Self::NW => &["M19 19 L5 5 M5 12 V5 H12"],
        };
        Icon { stroke, fill: &[] }
    }

    fn diagonal(self) -> bool {
        matches!(self, Self::NE | Self::SE | Self::SW | Self::NW)
    }
}

impl Shape {
    const fn new(legs: &[Direction]) -> Self {
        let mut shape = 0;
        let mut index = 0;
        while index < legs.len() {
            shape = (shape << 4) | legs[index] as u128;
            index += 1;
        }
        Self(shape)
    }

    fn directions(self) -> impl Iterator<Item = Direction> {
        (0..MAX_LEGS).rev().filter_map(move |index| {
            let code = ((self.0 >> (index * 4)) & 15) as u8;
            Direction::ALL
                .into_iter()
                .find_map(|(leg, _, _)| (code == leg as u8).then_some(leg))
        })
    }

    pub(in crate::app) fn for_row(action: &Action) -> Option<Self> {
        match *action {
            Action::Show(Screen::MultiswipeBinding(shape)) | Action::MultiswipePreview(shape) => {
                Some(shape)
            }
            _ => None,
        }
    }

    pub(in crate::app) fn draw(self, canvas: &mut Canvas, row: Rect) {
        let count = self.directions().count().max(1) as i32;
        let gap = canvas.px(3);
        let available = (row.w - canvas.px(56)).max(canvas.px(42));
        let size = ((available - gap * (count - 1)) / count)
            .clamp(canvas.px(8), canvas.px(42));
        let left = row.x + canvas.px(28);
        let top = row.y + canvas.px(19);
        let ink = if canvas.is_control_pressed(row) {
            PAPER
        } else {
            BLACK
        };
        for (index, direction) in self.directions().enumerate() {
            let arrow = Rect::new(left + index as i32 * (size + gap), top, size, size);
            canvas.icon(arrow, direction.icon(), ink);
        }
    }

    pub(in crate::app) fn arrows(self) -> String {
        self.directions()
            .map(|leg| Direction::ALL[leg as usize - 1].2)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl TryFrom<String> for Shape {
    type Error = &'static str;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        let mut words = value.split_ascii_whitespace();
        let Some(first) = words
            .next()
            .and_then(|word| Direction::ALL.into_iter().find(|entry| entry.1 == word))
            .map(|entry| entry.0)
        else {
            return Err("Invalid multiswipe directions");
        };
        let mut shape = Self::new(&[first]);
        let mut previous = first;
        let mut count = 1;
        for word in words {
            let leg = Direction::ALL
                .into_iter()
                .find_map(|(leg, name, _)| (word == name).then_some(leg))
                .filter(|leg| *leg != previous && leg.diagonal() == first.diagonal())
                .ok_or("Invalid multiswipe directions")?;
            if count == MAX_LEGS {
                return Err("Multiswipe has too many directions");
            }
            shape.0 = (shape.0 << 4) | leg as u128;
            previous = leg;
            count += 1;
        }
        if count < 2 {
            return Err("Multiswipe needs two directions");
        }
        Ok(shape)
    }
}

impl From<Shape> for String {
    fn from(shape: Shape) -> Self {
        shape
            .directions()
            .map(|leg| Direction::ALL[leg as usize - 1].1)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

use Direction::{E, N, NE, NW, S, SE, SW, W};

// Ordering follows the pinned reference's preset list.
const PRESETS: [(Shape, Binding); 35] = [
    (Shape::new(&[W, E]), Binding::Back),
    (Shape::new(&[E, W]), Binding::LatestBookmark),
    (Shape::new(&[N, S]), Binding::Ignore),
    (Shape::new(&[S, N]), Binding::Skim),
    (Shape::new(&[N, W]), Binding::Bookmarks),
    (Shape::new(&[N, E]), Binding::Contents),
    (Shape::new(&[S, W]), Binding::FrontLight),
    (Shape::new(&[S, E]), Binding::PassThrough),
    (Shape::new(&[E, N]), Binding::RecentBooks),
    (Shape::new(&[W, N]), Binding::BookDetails),
    (Shape::new(&[E, S]), Binding::GoTo),
    (Shape::new(&[W, S]), Binding::NavigationBack),
    (Shape::new(&[N, S, N]), Binding::PreviousChapter),
    (Shape::new(&[S, N, S]), Binding::NextChapter),
    (Shape::new(&[W, E, W]), Binding::PreviousBook),
    (Shape::new(&[E, W, E]), Binding::FavoriteBooks),
    (Shape::new(&[S, W, N]), Binding::PassThrough),
    (Shape::new(&[N, E, S]), Binding::PassThrough),
    (Shape::new(&[N, W, S]), Binding::PassThrough),
    (Shape::new(&[W, S, E]), Binding::PassThrough),
    (Shape::new(&[W, N, E]), Binding::PassThrough),
    (Shape::new(&[E, S, W]), Binding::PassThrough),
    (Shape::new(&[E, N, W]), Binding::PassThrough),
    (Shape::new(&[S, E, N]), Binding::PassThrough),
    (Shape::new(&[E, N, W, E]), Binding::FitPageWidth),
    (Shape::new(&[S, E, N, S]), Binding::FitPageHeight),
    (Shape::new(&[E, S, W, N]), Binding::Refresh),
    (Shape::new(&[W, S, E, N]), Binding::PassThrough),
    (Shape::new(&[S, E, N, W]), Binding::PassThrough),
    (Shape::new(&[S, W, N, E]), Binding::PassThrough),
    (Shape::new(&[SE, NE]), Binding::NearestLink),
    (Shape::new(&[NE, SE]), Binding::PassThrough),
    (Shape::new(&[NW, SW, NW]), Binding::PassThrough),
    (Shape::new(&[SE, SW, NW]), Binding::PassThrough),
    (Shape::new(&[SE, NE, NW]), Binding::PassThrough),
];

fn preset(shape: Shape) -> Option<Binding> {
    PRESETS
        .iter()
        .find_map(|&(key, action)| (key == shape).then_some(action))
}

struct Stroke {
    start: (f32, f32),
    leg_end: (f32, f32),
    last_direction: Option<Direction>,
    diagonal: bool,
    shape: Shape,
    count: usize,
    overflowed: bool,
    elapsed: Duration,
}

impl Stroke {
    fn new(start: (f32, f32)) -> Self {
        Self {
            start,
            leg_end: start,
            last_direction: None,
            diagonal: false,
            shape: Shape(0),
            count: 0,
            overflowed: false,
            elapsed: Duration::ZERO,
        }
    }

    fn sample(&mut self, end: (f32, f32)) {
        let dx = end.0 - self.leg_end.0;
        let dy = end.1 - self.leg_end.1;
        if dx.hypot(dy) <= MIN_LEG || self.overflowed {
            return;
        }
        if self.count == 0 {
            self.diagonal = dy.abs() > 0.577 * dx.abs() && dy.abs() < 1.732 * dx.abs();
        }
        let direction = Direction::infer(dx, dy, self.diagonal);
        if self.last_direction != Some(direction) {
            if self.count == MAX_LEGS {
                // Reject the whole overlong stroke. Keeping its first 32 legs could
                // accidentally invoke a configured action for a gesture the user did not draw.
                self.overflowed = true;
                return;
            }
            self.shape.0 = (self.shape.0 << 4) | direction as u128;
            self.count += 1;
            self.last_direction = Some(direction);
        }
        self.leg_end = end;
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct Profile {
    enabled: bool,
    assigned: BTreeMap<Shape, Binding>,
    custom: BTreeMap<Shape, Binding>,
    #[serde(default)]
    chains: BTreeMap<Shape, ActionChain>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::app) enum RunMode {
    #[default]
    ExecuteAll,
    OneByOne,
    QuickMenu,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::app) enum QuickMenuPosition {
    #[default]
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Gesture,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
struct ActionEntry {
    binding: Binding,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    separator_after: bool,
}

impl ActionEntry {
    fn new(binding: Binding) -> Self {
        Self {
            binding,
            name: None,
            separator_after: false,
        }
    }

    fn label(&self, shortcuts: &book_shortcuts::State) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.binding.label(shortcuts))
    }
}

impl QuickMenuPosition {
    const ALL: [(Self, &'static str); 10] = [
        (Self::Center, "Center"),
        (Self::Top, "Top"),
        (Self::Bottom, "Bottom"),
        (Self::Left, "Left"),
        (Self::Right, "Right"),
        (Self::TopLeft, "Top left"),
        (Self::TopRight, "Top right"),
        (Self::BottomLeft, "Bottom left"),
        (Self::BottomRight, "Bottom right"),
        (Self::Gesture, "Near gesture"),
    ];

    fn label(self) -> &'static str {
        Self::ALL
            .into_iter()
            .find_map(|(position, label)| (position == self).then_some(label))
            .unwrap_or("Center")
    }
}

impl Binding {
    fn icon(self) -> Icon {
        use super::super::bar_icons as icons;
        match self {
            Self::Default | Self::Ignore | Self::PassThrough | Self::PauseCornerHolds => icons::GESTURE,
            Self::Next | Self::Forward | Self::NextChapter | Self::NextTenPages => {
                icons::NEXT_PAGE
            }
            Self::Previous | Self::Back | Self::PreviousChapter | Self::PreviousTenPages => {
                icons::PREVIOUS_PAGE
            }
            Self::Contents | Self::BookDetails => icons::BOOK,
            Self::Bookmarks | Self::Bookmark | Self::LatestBookmark => icons::BOOKMARK,
            Self::Highlights => icons::HIGHLIGHT,
            Self::Skim => icons::GRID,
            Self::Find => icons::SEARCH,
            Self::Dictionary | Self::CycleDictionaryPresets => icons::BOOK,
            Self::GoTo => icons::JUMP,
            Self::NavigationBack => icons::BACK,
            Self::Refresh => icons::REFRESH,
            Self::Controls => icons::SLIDERS,
            Self::NearestLink => icons::LINK,
            Self::FrontLight => icons::DISPLAY,
            Self::RecentBooks | Self::PreviousBook | Self::BookShortcut(_) => icons::FOLDER,
            Self::StyleTweak(_) => icons::SLIDERS,
            Self::FavoriteBooks => icons::FAVORITE,
            Self::FitPageWidth => icons::FIT_WIDTH,
            Self::FitPageHeight => icons::FIT_HEIGHT,
            Self::IncreaseFont => icons::ZOOM_IN,
            Self::DecreaseFont => icons::ZOOM_OUT,
            Self::TogglePageScroll => icons::READFLOW,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
struct QuickMenuSettings {
    position: QuickMenuPosition,
    keep_open: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct ActionChain {
    actions: Vec<ActionEntry>,
    mode: RunMode,
    quick_menu: QuickMenuSettings,
}

impl Default for ActionChain {
    fn default() -> Self {
        Self {
            actions: Vec::new(),
            mode: RunMode::ExecuteAll,
            quick_menu: QuickMenuSettings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum QuickChoice {
    Stay,
    Close,
    Action(usize, Binding),
    All,
}

pub(in crate::app) struct QuickMenu {
    shape: Shape,
    actions: Vec<ActionEntry>,
    start: (f32, f32),
    anchor: (f32, f32),
    position: QuickMenuPosition,
    keep_open: bool,
    width: i32,
    height: i32,
    first: usize,
}

impl QuickMenu {
    fn new(
        shape: Shape,
        actions: Vec<ActionEntry>,
        start: (f32, f32),
        anchor: (f32, f32),
        width: i32,
        height: i32,
        position: QuickMenuPosition,
        keep_open: bool,
    ) -> Self {
        Self {
            shape,
            actions,
            start,
            anchor,
            position,
            keep_open,
            width,
            height,
            first: 0,
        }
    }

    fn layout(&self) -> (Rect, Rect, Rect, usize, i32) {
        let inset = 48;
        let panel_width = (self.width - inset * 2).clamp(280, 760);
        let panel_height = (self.height - inset * 2).clamp(340, 1200);
        let centered_x = (self.width - panel_width) / 2;
        let centered_y = (self.height - panel_height) / 2;
        let left = inset;
        let right = self.width - panel_width - inset;
        let top = inset;
        let bottom = self.height - panel_height - inset;
        let (x, y) = match self.position {
            QuickMenuPosition::Center => (centered_x, centered_y),
            QuickMenuPosition::Top => (centered_x, top),
            QuickMenuPosition::Bottom => (centered_x, bottom),
            QuickMenuPosition::Left => (left, centered_y),
            QuickMenuPosition::Right => (right, centered_y),
            QuickMenuPosition::TopLeft => (left, top),
            QuickMenuPosition::TopRight => (right, top),
            QuickMenuPosition::BottomLeft => (left, bottom),
            QuickMenuPosition::BottomRight => (right, bottom),
            QuickMenuPosition::Gesture => (
                (self.anchor.0 as i32 - panel_width / 2).clamp(left, right),
                (self.anchor.1 as i32 - panel_height / 2).clamp(top, bottom),
            ),
        };
        let panel = Rect::new(
            x,
            y,
            panel_width,
            panel_height,
        );
        let row_height = 66;
        let header = 92;
        let footer = 92;
        // Reserve a possible separator after every row so customized lists never
        // paint over the paging hint or the fixed Run all action.
        let visible = ((panel.h - header - footer) / (row_height + 24)).max(1) as usize;
        let run_all = Rect::new(
            panel.x + 24,
            panel.bottom() - footer + 10,
            panel.w - 48,
            68,
        );
        let close = Rect::new(panel.right() - 76, panel.y + 18, 52, 52);
        (panel, run_all, close, visible, row_height)
    }

    pub(in crate::app) fn draw(&self, canvas: &mut Canvas, shortcuts: &book_shortcuts::State) {
        let (panel, run_all, close, visible, row_height) = self.layout();
        canvas.fill(Rect::new(0, 0, self.width, self.height), 112);
        canvas.panel(panel, 18, 3, 255);
        let header = 92;
        canvas.text(
            Rect::new(
                panel.x + 24,
                panel.y + 15,
                panel.w - 112,
                62,
            ),
            &format!("{} · choose an action", self.shape.arrows()),
            crate::app::BODY.bold(),
        );
        canvas.icon(close, super::super::bar_icons::CLOSE, 30);
        let first = self.first.min(self.actions.len().saturating_sub(1));
        let mut y = panel.y + header;
        for index in first..(first + visible).min(self.actions.len()) {
            let row = Rect::new(
                panel.x + 18,
                y,
                panel.w - 36,
                row_height,
            );
            let pressed = canvas.is_control_pressed(row);
            if pressed {
                canvas.fill(row.inset(-3), 28);
            }
            canvas.text(
                Rect::new(row.x + 66, row.y, row.w - 82, row.h),
                &self.actions[index].label(shortcuts),
                crate::app::BODY,
            );
            canvas.icon(
                Rect::new(row.x + 18, row.y + (row.h - 36) / 2, 36, 36),
                self.actions[index].binding.icon(),
                30,
            );
            y += row_height;
            if self.actions[index].separator_after {
                canvas.fill(Rect::new(panel.x + 28, y + 5, panel.w - 56, 2), 150);
                y += 12;
            }
        }
        if self.actions.len() > visible {
            canvas.text(
                Rect::new(
                    panel.x + 24,
                    run_all.y - 48,
                    panel.w - 48,
                    42,
                ),
                &format!(
                    "{}–{} of {} · swipe to browse",
                    first + 1,
                    (first + visible).min(self.actions.len()),
                    self.actions.len()
                ),
                crate::app::CAPTION,
            );
        }
        canvas.button(run_all, "Run all actions", false, false);
    }

    pub(in crate::app) fn input(
        &mut self,
        input: primitive_ui::Input,
    ) -> QuickChoice {
        let (panel, run_all, close, visible, row_height) = self.layout();
        match input {
            primitive_ui::Input::Sleep | primitive_ui::Input::PageBack => QuickChoice::Close,
            primitive_ui::Input::PageForward => {
                if self.first + visible < self.actions.len() {
                    self.first = (self.first + visible).min(self.actions.len().saturating_sub(visible));
                }
                QuickChoice::Stay
            }
            primitive_ui::Input::Swipe { dx, dy, .. } if dy.abs() > dx.abs() => {
                if dy < 0.0 {
                    self.first = (self.first + visible).min(self.actions.len().saturating_sub(visible));
                } else {
                    self.first = self.first.saturating_sub(visible);
                }
                QuickChoice::Stay
            }
            primitive_ui::Input::Tap(x, y) if close.contains(x, y) => QuickChoice::Close,
            primitive_ui::Input::Tap(x, y) if run_all.contains(x, y) => QuickChoice::All,
            primitive_ui::Input::Tap(x, y) if !panel.contains(x, y) => QuickChoice::Close,
            primitive_ui::Input::Tap(x, y) => {
                let header = 92;
                if (y as i32) < panel.y + header {
                    return QuickChoice::Stay;
                }
                let end = (self.first + visible).min(self.actions.len());
                let mut row_top = panel.y + header;
                for index in self.first..end {
                    if (row_top..row_top + row_height).contains(&(y as i32)) {
                        return QuickChoice::Action(index, self.actions[index].binding);
                    }
                    row_top += row_height
                        + if self.actions[index].separator_after {
                            12
                        } else {
                            0
                        };
                }
                QuickChoice::Stay
            }
            _ => QuickChoice::Stay,
        }
    }
}

pub(in crate::app) struct Multiswipes {
    path: PathBuf,
    profile: Profile,
    stroke: Option<Stroke>,
    draft: Option<Shape>,
    cycle: BTreeMap<Shape, usize>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::app) enum Command {
    Toggle,
    Bind(Shape, Binding),
    AddAction(Shape, Binding),
    SetMode(Shape, RunMode),
    SetQuickMenuPosition(Shape, QuickMenuPosition),
    SetQuickMenuKeepOpen(Shape, bool),
    MoveAction(Shape, usize, bool),
    ToggleSeparator(Shape, usize),
    RemoveAction(Shape, usize),
    Save(Shape),
    Remove(Shape),
}

impl Multiswipes {
    pub(super) fn load(data: &Path) -> Self {
        let path = data.join("reading-multiswipes.json");
        let profile = read_import_file(&path, 128 * 1024)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            profile,
            stroke: None,
            draft: None,
            cycle: BTreeMap::new(),
        }
    }

    pub(in crate::app) fn enabled(&self) -> bool {
        self.profile.enabled
    }

    pub(in crate::app) fn clear_contact(&mut self) {
        self.stroke = None;
    }

    pub(in crate::app) fn clear_draft(&mut self) {
        self.draft = None;
    }

    fn can_record(&self) -> bool {
        self.enabled() && self.profile.custom.len() < MAX_CUSTOM
    }

    fn configured(&self, shape: Shape) -> Option<Binding> {
        if let Some(action) = self
            .profile
            .chains
            .get(&shape)
            .and_then(|chain| chain.actions.first())
        {
            return Some(action.binding);
        }
        let default = preset(shape);
        default
            .map(|action| self.profile.assigned.get(&shape).copied().unwrap_or(action))
            .or_else(|| self.profile.custom.get(&shape).copied())
    }

    fn configured_actions(&self, shape: Shape) -> Option<Vec<ActionEntry>> {
        self.profile
            .chains
            .get(&shape)
            .filter(|chain| !chain.actions.is_empty())
            .map(|chain| chain.actions.clone())
            .or_else(|| self.configured(shape).map(|action| vec![ActionEntry::new(action)]))
    }

    fn run_mode(&self, shape: Shape) -> RunMode {
        self.profile
            .chains
            .get(&shape)
            .map_or(RunMode::ExecuteAll, |chain| chain.mode)
    }

    fn next_cycle_action(&mut self, shape: Shape) -> Option<Binding> {
        let actions = self.configured_actions(shape)?;
        if actions.is_empty() {
            return None;
        }
        let next = self.cycle.entry(shape).or_default();
        let action = actions[*next % actions.len()].binding;
        *next = (*next + 1) % actions.len();
        Some(action)
    }

    fn chain_for(profile: &Profile, shape: Shape) -> Option<ActionChain> {
        if let Some(chain) = profile.chains.get(&shape).filter(|chain| !chain.actions.is_empty()) {
            return Some(chain.clone());
        }
        preset(shape)
            .map(|action| {
                profile
                    .assigned
                    .get(&shape)
                    .copied()
                    .unwrap_or(action)
            })
            .or_else(|| profile.custom.get(&shape).copied())
            .map(|action| ActionChain {
                actions: vec![ActionEntry::new(action)],
                mode: RunMode::ExecuteAll,
                quick_menu: QuickMenuSettings::default(),
            })
    }

    fn quick_menu_settings(&self, shape: Shape) -> QuickMenuSettings {
        self.profile
            .chains
            .get(&shape)
            .map_or_else(QuickMenuSettings::default, |chain| chain.quick_menu)
    }

    pub(in crate::app) fn rename_action(
        &mut self,
        shape: Shape,
        index: usize,
        name: &str,
    ) -> Result<()> {
        let mut profile = self.profile.clone();
        let mut chain = Self::chain_for(&profile, shape).ok_or("Multiswipe no longer exists")?;
        let action = chain
            .actions
            .get_mut(index)
            .ok_or("Action no longer exists")?;
        let cleaned = name
            .chars()
            .filter(|character| !character.is_control())
            .take(40)
            .collect::<String>();
        let trimmed = cleaned.trim();
        action.name = (!trimmed.is_empty()).then(|| trimmed.to_owned());
        profile.chains.insert(shape, chain);
        primitive_app::save(&self.path, &serde_json::to_vec(&profile)?)?;
        self.profile = profile;
        Ok(())
    }

    pub(in crate::app) fn action_name(&self, shape: Shape, index: usize) -> Option<&str> {
        self.profile
            .chains
            .get(&shape)?
            .actions
            .get(index)?
            .name
            .as_deref()
    }

    fn observe(&mut self, pan: TouchPan) -> Option<Shape> {
        if pan.phase == PanPhase::Cancel
            || pan.elapsed >= SWIPE_WINDOW
            || ![pan.start.0, pan.start.1, pan.end.0, pan.end.1]
                .iter()
                .all(|point| point.is_finite())
        {
            self.clear_contact();
            return None;
        }
        if pan.phase == PanPhase::Move {
            let mut stroke = self
                .stroke
                .take()
                .filter(|stroke| stroke.start == pan.start && stroke.elapsed <= pan.elapsed)
                .unwrap_or_else(|| Stroke::new(pan.start));
            stroke.sample(pan.end);
            stroke.elapsed = pan.elapsed;
            self.stroke = Some(stroke);
            return None;
        }
        let mut stroke = self.stroke.take()?;
        if stroke.start != pan.start || pan.elapsed < stroke.elapsed || pan.start == pan.end {
            return None;
        }
        // The toolkit retains the final down sample even when its move was coalesced.
        stroke.sample(pan.end);
        (!stroke.overflowed && (2..=MAX_LEGS).contains(&stroke.count))
            .then_some(stroke.shape)
    }

    fn save(&mut self, command: &Command) -> Result<Screen> {
        let mut profile = self.profile.clone();
        if !profile.enabled && !matches!(command, Command::Toggle) {
            return Err("Turn on multiswipes first".into());
        }
        let screen = match command {
            Command::Toggle => {
                profile.enabled = !profile.enabled;
                Screen::Gestures
            }
            Command::Bind(shape, action) if preset(*shape).is_some() => {
                if *action == Binding::Default {
                    profile.assigned.remove(shape);
                } else {
                    profile.assigned.insert(*shape, *action);
                }
                profile.chains.remove(shape);
                self.cycle.remove(shape);
                Screen::Multiswipes
            }
            Command::Bind(shape, action) => {
                let entry = profile
                    .custom
                    .get_mut(shape)
                    .ok_or("Multiswipe no longer exists")?;
                *entry = if *action == Binding::Default {
                    Binding::PassThrough
                } else {
                    *action
                };
                profile.chains.remove(shape);
                self.cycle.remove(shape);
                Screen::CustomMultiswipes
            }
            Command::AddAction(shape, action) => {
                if *action == Binding::Default {
                    return Err("Choose a reading action".into());
                }
                let mut chain = Self::chain_for(&profile, *shape)
                    .ok_or("Multiswipe no longer exists")?;
                if chain.actions.len() >= MAX_ACTIONS {
                    return Err("A multiswipe can have up to 16 actions".into());
                }
                chain.actions.push(ActionEntry::new(*action));
                profile.chains.insert(*shape, chain.clone());
                Self::set_primary_action(&mut profile, *shape, chain.actions[0].binding);
                self.cycle.remove(shape);
                Screen::MultiswipeActions(*shape)
            }
            Command::SetMode(shape, mode) => {
                let mut chain = Self::chain_for(&profile, *shape)
                    .ok_or("Multiswipe no longer exists")?;
                chain.mode = *mode;
                profile.chains.insert(*shape, chain.clone());
                Self::set_primary_action(&mut profile, *shape, chain.actions[0].binding);
                self.cycle.remove(shape);
                Screen::MultiswipeActions(*shape)
            }
            Command::SetQuickMenuPosition(shape, position) => {
                let mut chain = Self::chain_for(&profile, *shape)
                    .ok_or("Multiswipe no longer exists")?;
                chain.quick_menu.position = *position;
                profile.chains.insert(*shape, chain);
                Screen::MultiswipeQuickMenuSettings(*shape)
            }
            Command::SetQuickMenuKeepOpen(shape, keep_open) => {
                let mut chain = Self::chain_for(&profile, *shape)
                    .ok_or("Multiswipe no longer exists")?;
                chain.quick_menu.keep_open = *keep_open;
                profile.chains.insert(*shape, chain);
                Screen::MultiswipeQuickMenuSettings(*shape)
            }
            Command::MoveAction(shape, index, up) => {
                let mut chain = Self::chain_for(&profile, *shape)
                    .ok_or("Multiswipe no longer exists")?;
                if *index >= chain.actions.len() {
                    return Err("Action no longer exists".into());
                }
                let destination = if *up {
                    index.checked_sub(1)
                } else {
                    index.checked_add(1).filter(|next| *next < chain.actions.len())
                };
                let Some(destination) = destination.filter(|next| *next < chain.actions.len()) else {
                    return Err("That action cannot move in this direction".into());
                };
                chain.actions.swap(*index, destination);
                profile.chains.insert(*shape, chain.clone());
                Self::set_primary_action(&mut profile, *shape, chain.actions[0].binding);
                self.cycle.remove(shape);
                Screen::MultiswipeActions(*shape)
            }
            Command::ToggleSeparator(shape, index) => {
                let mut chain = Self::chain_for(&profile, *shape)
                    .ok_or("Multiswipe no longer exists")?;
                let action = chain
                    .actions
                    .get_mut(*index)
                    .ok_or("Action no longer exists")?;
                action.separator_after = !action.separator_after;
                profile.chains.insert(*shape, chain);
                Screen::EditMultiswipeAction(*shape, *index)
            }
            Command::RemoveAction(shape, index) => {
                let mut chain = Self::chain_for(&profile, *shape)
                    .ok_or("Multiswipe no longer exists")?;
                if chain.actions.len() <= 1 || *index >= chain.actions.len() {
                    return Err("A multiswipe needs at least one action".into());
                }
                chain.actions.remove(*index);
                profile.chains.insert(*shape, chain.clone());
                Self::set_primary_action(&mut profile, *shape, chain.actions[0].binding);
                self.cycle.remove(shape);
                Screen::MultiswipeActions(*shape)
            }
            Command::Save(shape) => {
                if self.draft != Some(*shape)
                    || !self.can_record()
                    || self.configured(*shape).is_some()
                {
                    return Err("This multiswipe cannot be saved".into());
                }
                profile.custom.insert(*shape, Binding::PassThrough);
                Screen::MultiswipeBinding(*shape)
            }
            Command::Remove(shape) => {
                profile
                    .custom
                    .remove(shape)
                    .ok_or("Multiswipe no longer exists")?;
                profile.chains.remove(shape);
                self.cycle.remove(shape);
                Screen::CustomMultiswipes
            }
        };
        primitive_app::save(&self.path, &serde_json::to_vec(&profile)?)?;
        self.profile = profile;
        Ok(screen)
    }

    fn set_primary_action(profile: &mut Profile, shape: Shape, action: Binding) {
        if preset(shape).is_some() {
            profile.assigned.insert(shape, action);
        } else {
            profile.custom.insert(shape, action);
        }
    }

    fn binding_item(&self, shape: Shape, shortcuts: &book_shortcuts::State) -> Item {
        Item::new(
            shape.arrows(),
            Action::Show(Screen::MultiswipeBinding(shape)),
        )
        .detail(self.summary(shape, shortcuts))
    }

    fn summary(&self, shape: Shape, shortcuts: &book_shortcuts::State) -> String {
        let actions = self.configured_actions(shape).unwrap_or_default();
        let mut summary = actions
            .iter()
            .take(3)
            .map(|action| action.binding.label(shortcuts))
            .collect::<Vec<_>>()
            .join(" → ");
        if actions.len() > 3 {
            summary.push_str(&format!(" → +{} more", actions.len() - 3));
        }
        if actions.len() > 1 || self.run_mode(shape) != RunMode::ExecuteAll {
            let mode = match self.run_mode(shape) {
                RunMode::ExecuteAll => "all",
                RunMode::OneByOne => "one by one",
                RunMode::QuickMenu => "quick menu",
            };
            summary.push_str(" · ");
            summary.push_str(mode);
        }
        summary
    }

    pub(in crate::app) fn items(
        &self,
        screen: Screen,
        shortcuts: &book_shortcuts::State,
    ) -> Vec<Item> {
        if !self.enabled() {
            return Vec::new();
        }
        match screen {
            Screen::Multiswipes => PRESETS
                .into_iter()
                .map(|(shape, _)| self.binding_item(shape, shortcuts))
                .collect(),
            Screen::CustomMultiswipes => {
                let mut items = vec![if self.can_record() {
                    Item::new(
                        "Record a multiswipe",
                        Action::Show(Screen::RecordMultiswipe),
                    )
                } else {
                    Item::new("Custom multiswipe limit reached", Action::Heading)
                        .detail("Remove a multiswipe to record another")
                }];
                items.extend(
                    self.profile
                        .custom
                        .keys()
                        .map(|&shape| self.binding_item(shape, shortcuts)),
                );
                items
            }
            Screen::RecordMultiswipe => match self.draft {
                Some(shape) => {
                    let mut items = vec![
                        Item::new(shape.arrows(), Action::MultiswipePreview(shape)).detail(
                            if self.configured(shape).is_some() {
                                "Already exists. Draw another shape."
                            } else {
                                "Draw again to replace this shape"
                            },
                        ),
                    ];
                    if self.configured(shape).is_none() {
                        items.push(Item::new(
                            "Save multiswipe",
                            Action::Multiswipe(Command::Save(shape)),
                        ));
                    }
                    items
                }
                None => vec![
                    Item::new("Draw a shape here", Action::Heading)
                        .detail("Keep one finger down and change direction"),
                ],
            },
            Screen::MultiswipeBinding(shape) => {
                let is_preset = preset(shape).is_some();
                let selected = self
                    .profile
                    .assigned
                    .get(&shape)
                    .copied()
                    .or_else(|| self.profile.custom.get(&shape).copied())
                    .unwrap_or(Binding::Default);
                let mut items = Binding::ALL
                    .into_iter()
                    .filter(|(action, _)| is_preset || *action != Binding::Default)
                    .map(|(action, title)| {
                        let item =
                            Item::new(title, Action::Multiswipe(Command::Bind(shape, action)));
                        if action == selected {
                            item.detail("Selected")
                        } else {
                            item
                        }
                    })
                    .collect::<Vec<_>>();
                items.extend(shortcuts.binding_items(selected, |action| {
                    Action::Multiswipe(Command::Bind(shape, action))
                }));
                if !is_preset {
                    items.insert(
                        0,
                        Item::new(
                            "Remove multiswipe",
                            Action::Show(Screen::RemoveMultiswipe(shape)),
                        ),
                    );
                }
                items.insert(
                    0,
                    Item::new(shape.arrows(), Action::MultiswipePreview(shape)).detail(
                        self.summary(shape, shortcuts),
                    ),
                );
                items.insert(
                    1,
                    Item::new(
                        "Manage action sequence",
                        Action::Show(Screen::MultiswipeActions(shape)),
                    )
                    .detail("Order actions and choose how this gesture runs them"),
                );
                items
            }
            Screen::MultiswipeActions(shape) => {
                let Some(chain) = Self::chain_for(&self.profile, shape) else {
                    return vec![Item::new("Multiswipe no longer exists", Action::Heading)];
                };
                let mode = match chain.mode {
                    RunMode::ExecuteAll => "Run all actions in order",
                    RunMode::OneByOne => "Run one action per gesture",
                    RunMode::QuickMenu => "Ask with a quick menu",
                };
                let mut items = vec![
                    Item::new(shape.arrows(), Action::MultiswipePreview(shape))
                        .detail(format!("{} action(s)", chain.actions.len())),
                    Item::new(mode, Action::Show(Screen::MultiswipeMode(shape)))
                        .detail("Change gesture behavior"),
                    if chain.actions.len() < MAX_ACTIONS {
                        Item::new(
                            "Add action",
                            Action::Show(Screen::AddMultiswipeAction(shape)),
                        )
                        .detail(format!("Up to {MAX_ACTIONS} actions"))
                    } else {
                        Item::new("Action limit reached", Action::Heading)
                            .detail(format!("A multiswipe supports {MAX_ACTIONS} actions"))
                    },
                ];
                if chain.mode == RunMode::QuickMenu {
                    let quick = chain.quick_menu;
                    items.push(
                        Item::new(
                            "Quick menu options",
                            Action::Show(Screen::MultiswipeQuickMenuSettings(shape)),
                        )
                        .detail(format!(
                            "{} · {}",
                            quick.position.label(),
                            if quick.keep_open { "stays open" } else { "closes after choice" }
                        )),
                    );
                }
                items.extend(chain.actions.iter().enumerate().map(|(index, action)| {
                    Item::new(
                        action.label(shortcuts),
                        Action::Show(Screen::EditMultiswipeAction(shape, index)),
                    )
                    .detail(format!("Action {} · tap to reorder or remove", index + 1))
                }));
                items
            }
            Screen::AddMultiswipeAction(shape) => {
                let mut items = Binding::ALL
                    .into_iter()
                    .filter(|(action, _)| *action != Binding::Default)
                    .map(|(action, title)| {
                        Item::new(title, Action::Multiswipe(Command::AddAction(shape, action)))
                    })
                    .collect::<Vec<_>>();
                items.extend(shortcuts.binding_items(Binding::Default, |action| {
                    Action::Multiswipe(Command::AddAction(shape, action))
                }));
                items
            }
            Screen::MultiswipeMode(shape) => {
                let selected = self.run_mode(shape);
                [
                    (RunMode::ExecuteAll, "Run all actions in order", "Every assigned action runs when the gesture is recognized"),
                    (RunMode::OneByOne, "Run one action per gesture", "Repeat the gesture to cycle through its actions"),
                    (RunMode::QuickMenu, "Ask with a quick menu", "Choose one action or run the full sequence"),
                ]
                .into_iter()
                .map(|(mode, title, detail)| {
                    let item = Item::new(
                        title,
                        Action::Multiswipe(Command::SetMode(shape, mode)),
                    )
                    .detail(detail);
                    if mode == selected { item.detail("Selected") } else { item }
                })
                .collect()
            }
            Screen::MultiswipeQuickMenuSettings(shape) => {
                let settings = self.quick_menu_settings(shape);
                vec![
                    Item::new(
                        format!("Position · {}", settings.position.label()),
                        Action::Show(Screen::MultiswipeQuickMenuPosition(shape)),
                    ),
                    Item::new(
                        if settings.keep_open {
                            "Keep open after choosing · On"
                        } else {
                            "Keep open after choosing · Off"
                        },
                        Action::Multiswipe(Command::SetQuickMenuKeepOpen(
                            shape,
                            !settings.keep_open,
                        )),
                    )
                    .detail("Applies when an action keeps the current reading view open"),
                ]
            }
            Screen::MultiswipeQuickMenuPosition(shape) => {
                let selected = self.quick_menu_settings(shape).position;
                QuickMenuPosition::ALL
                    .into_iter()
                    .map(|(position, label)| {
                        let item = Item::new(
                            label,
                            Action::Multiswipe(Command::SetQuickMenuPosition(shape, position)),
                        );
                        if position == selected {
                            item.detail("Selected")
                        } else {
                            item
                        }
                    })
                    .collect()
            }
            Screen::EditMultiswipeAction(shape, index) => {
                let Some(chain) = Self::chain_for(&self.profile, shape) else {
                    return vec![Item::new("Multiswipe no longer exists", Action::Heading)];
                };
                let Some(action) = chain.actions.get(index).cloned() else {
                    return vec![Item::new("Action no longer exists", Action::Heading)];
                };
                let mut items = vec![
                    Item::new(action.label(shortcuts), Action::MultiswipePreview(shape))
                        .detail(format!("Action {} of {}", index + 1, chain.actions.len())),
                ];
                if index > 0 {
                    items.push(Item::new(
                        "Move up",
                        Action::Multiswipe(Command::MoveAction(shape, index, true)),
                    ));
                }
                if index + 1 < chain.actions.len() {
                    items.push(Item::new(
                        "Move down",
                        Action::Multiswipe(Command::MoveAction(shape, index, false)),
                    ));
                }
                if chain.mode == RunMode::QuickMenu {
                    items.push(Item::new(
                        "Rename quick menu action",
                        Action::Show(Screen::MultiswipeActionName(shape, index)),
                    ));
                    items.push(
                        Item::new(
                            if action.separator_after {
                                "Separator after action · On"
                            } else {
                                "Separator after action · Off"
                            },
                            Action::Multiswipe(Command::ToggleSeparator(shape, index)),
                        )
                        .detail("Adds a dividing line in the quick menu"),
                    );
                }
                if chain.actions.len() > 1 {
                    items.push(
                        Item::new(
                            "Remove action",
                            Action::Multiswipe(Command::RemoveAction(shape, index)),
                        )
                        .detail("The other actions will remain assigned"),
                    );
                }
                items
            }
            Screen::RemoveMultiswipe(shape) => vec![
                Item::new(shape.arrows(), Action::MultiswipePreview(shape))
                    .detail("The assigned action will be removed too"),
                Item::new(
                    "Remove multiswipe",
                    Action::Multiswipe(Command::Remove(shape)),
                ),
            ],
            _ => Vec::new(),
        }
    }

    fn overview(&self, shortcuts: &book_shortcuts::State) -> Vec<Item> {
        if !self.enabled() {
            return Vec::new();
        }
        PRESETS
            .iter()
            .map(|&(shape, _)| shape)
            .chain(self.profile.custom.keys().copied())
            .filter_map(|shape| {
                let actions = self.configured_actions(shape)?;
                actions
                    .iter()
                    .any(|action| {
                        !matches!(action.binding, Binding::PassThrough | Binding::Ignore)
                    })
                    .then(|| {
                        Item::new(shape.arrows(), Action::MultiswipePreview(shape))
                            .detail(self.summary(shape, shortcuts))
                    })
            })
            .collect()
    }

    pub(in crate::app) fn list_for(shape: Shape) -> Screen {
        if preset(shape).is_some() {
            Screen::Multiswipes
        } else {
            Screen::CustomMultiswipes
        }
    }
}

impl Reader {
    pub(in crate::app) fn activate_multiswipe(&mut self, command: Command) -> Result<()> {
        let screen = self.gestures.multiswipes.save(&command)?;
        if let Command::Bind(shape, _) = command {
            self.return_to_multiswipe_list(shape);
        } else {
            self.show(screen);
        }
        Ok(())
    }

    fn run_multiswipe_actions(
        &mut self,
        actions: &[ActionEntry],
        location: Option<(f32, f32)>,
    ) -> Option<Change> {
        let mut result = None;
        for action in actions {
            if let Some(change) = self.dispatch_gesture_binding(action.binding, location) {
                result = Some(
                    result.map_or(change, |previous| super::merge_changes(previous, change)),
                );
                if result == Some(Change::Exit) {
                    break;
                }
            }
        }
        result
    }

    pub(in crate::app) fn multiswipe_menu_input(
        &mut self,
        input: primitive_ui::Input,
    ) -> Option<Change> {
        if input == primitive_ui::Input::Sleep {
            self.multiswipe_menu = None;
            return None;
        }
        if input == primitive_ui::Input::Wake {
            return None;
        }
        let menu = self.multiswipe_menu.as_mut()?;
        let choice = menu.input(input);
        if choice == QuickChoice::Stay {
            return Some(Change::Partial);
        }
        let menu = self.multiswipe_menu.take()?;
        Some(match choice {
            QuickChoice::Stay => Change::Partial,
            QuickChoice::Close => Change::Partial,
            QuickChoice::Action(_, action) => {
                let initial_screen = self.screen;
                let change = self.dispatch_gesture_binding(action, Some(menu.start));
                if menu.keep_open
                    && change != Some(Change::Exit)
                    && self.screen == initial_screen
                    && matches!(self.screen, Screen::Book | Screen::Fixed)
                {
                    self.multiswipe_menu = Some(menu);
                }
                match change {
                    Some(Change::None) | None => Change::Partial,
                    Some(change) => change,
                }
            }
            QuickChoice::All => match self.run_multiswipe_actions(&menu.actions, Some(menu.start)) {
                Some(Change::None) | None => Change::Partial,
                Some(change) => change,
            },
        })
    }

    pub(in crate::app) fn return_to_multiswipe_list(&mut self, shape: Shape) {
        let screen = Multiswipes::list_for(shape);
        self.show(screen);
        if let Some(index) = self.gestures.multiswipes.items(screen, &self.book_shortcuts).iter().position(
            |item| matches!(item.action, Action::Show(Screen::MultiswipeBinding(key)) if key == shape),
        ) {
            self.list_page = index / self.list_geometry().rows;
        }
    }

    pub(in crate::app) fn gesture_overview_items(&self) -> Vec<Item> {
        let mut items = Gesture::ALL
            .into_iter()
            .filter_map(|(gesture, title)| {
                let binding = self.gestures.get(gesture);
                (!matches!(
                    binding,
                    Binding::Default | Binding::Ignore | Binding::PassThrough
                ))
                .then(|| {
                    Item::new(title, Action::Heading).detail(binding.label(&self.book_shortcuts))
                })
            })
            .collect::<Vec<_>>();
        items.extend(self.gestures.multiswipes.overview(&self.book_shortcuts));
        items
    }

    pub(in crate::app) fn recognize_multiswipe(&mut self, pan: TouchPan) -> Option<Change> {
        let recording = self.screen == Screen::RecordMultiswipe;
        let viewport = self.reading_viewport();
        let (width, height) = viewport.dimensions();
        let (x, y) = pan.start;
        if !self.gestures.multiswipes.enabled()
            || !self.stats_awake
            || self.touch_locked
            || !(0.0..width as f32).contains(&x)
            || !(0.0..height as f32).contains(&y)
            || (!recording
                && (!self.touch_selection_enabled()
                    || self.selection.is_some()
                    || self.pdf_selection.is_some()
                    || self.selection_mode
                    || self.touch_hold.is_some()
                    || self.fixed_search_fit_active()
                    || self.opening_ask_visible()
                    || self.pending_opening_profiles.is_some()
                    || self.closing_profile.is_some()
                    || self.back_exit_confirmation_open()))
        {
            self.gestures.multiswipes.clear_contact();
            return None;
        }
        let shape = self.gestures.multiswipes.observe(pan);
        if recording {
            return Some(match shape {
                Some(shape) => {
                    self.gestures.multiswipes.draft = Some(shape);
                    Change::Partial
                }
                None => Change::None,
            });
        }
        let shape = shape?;
        let mode = self.gestures.multiswipes.run_mode(shape);
        let actions = self.gestures.multiswipes.configured_actions(shape)?;
        if actions.len() == 1 && actions[0].binding == Binding::PassThrough {
            return None;
        }
        if mode == RunMode::QuickMenu {
            self.continuous_touch_pan(TouchPan {
                phase: PanPhase::Cancel,
                ..pan
            });
            let (width, height) = viewport.dimensions();
            let settings = self.gestures.multiswipes.quick_menu_settings(shape);
            self.multiswipe_menu = Some(QuickMenu::new(
                shape,
                actions,
                pan.start,
                pan.end,
                width as i32,
                height as i32,
                settings.position,
                settings.keep_open,
            ));
            return Some(Change::Partial);
        }
        let actions = if mode == RunMode::OneByOne {
            vec![ActionEntry::new(
                self.gestures.multiswipes.next_cycle_action(shape)?,
            )]
        } else {
            actions
        };
        let restored = self.continuous_touch_pan(TouchPan {
            phase: PanPhase::Cancel,
            ..pan
        });
        match self.run_multiswipe_actions(&actions, Some(pan.start)) {
            None | Some(Change::None) => Some(restored),
            change => change,
        }
    }
}

#[cfg(test)]
mod tests;
