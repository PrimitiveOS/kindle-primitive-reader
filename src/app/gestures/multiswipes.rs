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

const MAX_LEGS: usize = 8;
const MAX_CUSTOM: usize = 64;
// KOReader's threshold is ceil(50 × 300 / 160) at the SDK's Oasis reference DPI.
const MIN_LEG: f32 = 94.0;
const SWIPE_WINDOW: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(in crate::app) struct Shape(u32);

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
            shape = (shape << 4) | legs[index] as u32;
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
        let size = canvas.px(42);
        let gap = canvas.px(11);
        let left = row.x + canvas.px(32);
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
            shape.0 = (shape.0 << 4) | leg as u32;
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
            elapsed: Duration::ZERO,
        }
    }

    fn sample(&mut self, end: (f32, f32)) {
        let dx = end.0 - self.leg_end.0;
        let dy = end.1 - self.leg_end.1;
        if dx.hypot(dy) <= MIN_LEG || self.count > MAX_LEGS {
            return;
        }
        if self.count == 0 {
            self.diagonal = dy.abs() > 0.577 * dx.abs() && dy.abs() < 1.732 * dx.abs();
        }
        let direction = Direction::infer(dx, dy, self.diagonal);
        if self.last_direction != Some(direction) {
            self.shape.0 = (self.shape.0 << 4) | direction as u32;
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
}

pub(in crate::app) struct Multiswipes {
    path: PathBuf,
    profile: Profile,
    stroke: Option<Stroke>,
    draft: Option<Shape>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::app) enum Command {
    Toggle,
    Bind(Shape, Binding),
    Save(Shape),
    Remove(Shape),
}

impl Multiswipes {
    pub(super) fn load(data: &Path) -> Self {
        let path = data.join("reading-multiswipes.json");
        let profile = read_import_file(&path, 32 * 1024)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            profile,
            stroke: None,
            draft: None,
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
        let default = preset(shape);
        default
            .map(|action| self.profile.assigned.get(&shape).copied().unwrap_or(action))
            .or_else(|| self.profile.custom.get(&shape).copied())
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
        (2..=MAX_LEGS)
            .contains(&stroke.count)
            .then_some(stroke.shape)
    }

    fn save(&mut self, command: Command) -> Result<Screen> {
        let mut profile = self.profile.clone();
        if !profile.enabled && !matches!(command, Command::Toggle) {
            return Err("Turn on multiswipes first".into());
        }
        let screen = match command {
            Command::Toggle => {
                profile.enabled = !profile.enabled;
                Screen::Gestures
            }
            Command::Bind(shape, action) if preset(shape).is_some() => {
                if action == Binding::Default {
                    profile.assigned.remove(&shape);
                } else {
                    profile.assigned.insert(shape, action);
                }
                Screen::Multiswipes
            }
            Command::Bind(shape, action) => {
                let entry = profile
                    .custom
                    .get_mut(&shape)
                    .ok_or("Multiswipe no longer exists")?;
                *entry = if action == Binding::Default {
                    Binding::PassThrough
                } else {
                    action
                };
                Screen::CustomMultiswipes
            }
            Command::Save(shape) => {
                if self.draft != Some(shape)
                    || !self.can_record()
                    || self.configured(shape).is_some()
                {
                    return Err("This multiswipe cannot be saved".into());
                }
                profile.custom.insert(shape, Binding::PassThrough);
                Screen::MultiswipeBinding(shape)
            }
            Command::Remove(shape) => {
                profile
                    .custom
                    .remove(&shape)
                    .ok_or("Multiswipe no longer exists")?;
                Screen::CustomMultiswipes
            }
        };
        primitive_app::save(&self.path, &serde_json::to_vec(&profile)?)?;
        self.profile = profile;
        Ok(screen)
    }

    fn binding_item(&self, shape: Shape, shortcuts: &book_shortcuts::State) -> Item {
        Item::new(
            shape.arrows(),
            Action::Show(Screen::MultiswipeBinding(shape)),
        )
        .detail(
            self.configured(shape)
                .unwrap_or(Binding::PassThrough)
                .label(shortcuts),
        )
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
                        self.configured(shape)
                            .unwrap_or(Binding::PassThrough)
                            .label(shortcuts),
                    ),
                );
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
                let action = self.configured(shape)?;
                (!matches!(action, Binding::PassThrough | Binding::Ignore)).then(|| {
                    Item::new(shape.arrows(), Action::MultiswipePreview(shape))
                        .detail(action.label(shortcuts))
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
        let screen = self.gestures.multiswipes.save(command)?;
        if let Command::Bind(shape, _) = command {
            self.return_to_multiswipe_list(shape);
        } else {
            self.show(screen);
        }
        Ok(())
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
        // The content-only region leaves back, the visible footer, launcher and reading edge
        // gestures, and the edge light zones available. Recorder gestures follow these limits too.
        let top = viewport.page_height() as f32 / 5.0;
        let bottom = height as f32 - top;
        let (x, y) = pan.start;
        if !self.gestures.multiswipes.enabled()
            || !self.stats_awake
            || self.touch_locked
            || !(100.0..width as f32 - 100.0).contains(&x)
            || !(top..bottom).contains(&y)
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
        let action = self.gestures.multiswipes.configured(shape?)?;
        if action == Binding::PassThrough {
            return None;
        }
        let restored = self.continuous_touch_pan(TouchPan {
            phase: PanPhase::Cancel,
            ..pan
        });
        match self.dispatch_gesture_binding(action, Some(pan.start)) {
            None | Some(Change::None) => Some(restored),
            change => change,
        }
    }
}

#[cfg(test)]
mod tests;
