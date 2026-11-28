//! Assign touch gestures to existing Reader actions. The SDK owns system gestures.
use super::book_shortcuts;
use super::{Action, BookList, DisplayLevel, Item, Reader, Screen, TopGroup, read_import_file};

pub(super) mod multiswipes;
use crate::{Result, reading_controls::ReadingControls, session::ReadingViewport};
use primitive_app::{App, Change};
#[cfg(feature = "multi-touch")]
use primitive_ui::MultiTouch;
use primitive_ui::Input;
use serde::{Deserialize, Serialize};

/// Two-finger input, mirroring `primitive_ui::MultiTouch`. The shared kit publishes that type
/// only in tools revisions after the reader's pin, so the default build uses this copy and keeps
/// the two-finger gesture code and its tests compiled. Enable `multi-touch` once the pinned tools
/// revision provides `primitive_ui::MultiTouch` and `App::multi_touch`.
#[cfg(not(feature = "multi-touch"))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MultiTouch {
    TwoFingerTap {
        x: f32,
        y: f32,
        held_ms: u32,
        fallback_x: f32,
        fallback_y: f32,
    },
    TwoFingerSwipe {
        dx: f32,
        dy: f32,
        start_x: f32,
        start_y: f32,
    },
    Spread,
    Pinch,
    RotateClockwise,
    RotateCounterclockwise,
}
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) const CORNER_PAUSE_RANGE: std::ops::RangeInclusive<u8> = 5..=30;
const DOUBLE_TAP_DISTANCE_PX: f32 = 50.0;
const TWO_FINGER_HOLD_INTERVAL_MS: u32 = 500;

pub(super) fn corner_pause_seconds(controls: &ReadingControls) -> u8 {
    controls
        .corner_hold_pause_seconds
        .filter(|value| CORNER_PAUSE_RANGE.contains(value))
        .unwrap_or(10)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Gesture {
    TapTopLeft,
    TapTopRight,
    TapBottomLeft,
    TapBottomRight,
    HoldTopLeft,
    HoldTopRight,
    HoldBottomLeft,
    HoldBottomRight,
    LeftUp,
    LeftDown,
    RightUp,
    RightDown,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    ShortDiagonal,
    DoubleTapLeft,
    DoubleTapRight,
    TwoFingerTapTopLeft,
    TwoFingerTapTopRight,
    TwoFingerTapBottomLeft,
    TwoFingerTapBottomRight,
    TwoFingerSwipeNorth,
    TwoFingerSwipeNorthEast,
    TwoFingerSwipeEast,
    TwoFingerSwipeSouthEast,
    TwoFingerSwipeSouth,
    TwoFingerSwipeSouthWest,
    TwoFingerSwipeWest,
    TwoFingerSwipeNorthWest,
    Spread,
    Pinch,
    RotateClockwise,
    RotateCounterclockwise,
}

impl Gesture {
    pub(super) const ALL: [(Self, &'static str); 35] = [
        (Self::TapTopLeft, "Tap upper left"),
        (Self::TapTopRight, "Tap upper right"),
        (Self::TapBottomLeft, "Tap lower left"),
        (Self::TapBottomRight, "Tap lower right"),
        (Self::HoldTopLeft, "Hold upper left"),
        (Self::HoldTopRight, "Hold upper right"),
        (Self::HoldBottomLeft, "Hold lower left"),
        (Self::HoldBottomRight, "Hold lower right"),
        (Self::LeftUp, "Left edge up"),
        (Self::LeftDown, "Left edge down"),
        (Self::RightUp, "Right edge up"),
        (Self::RightDown, "Right edge down"),
        (Self::TopLeft, "Top edge left"),
        (Self::TopRight, "Top edge right"),
        (Self::BottomLeft, "Bottom edge left"),
        (Self::BottomRight, "Bottom edge right"),
        (Self::ShortDiagonal, "Short diagonal swipe"),
        (Self::DoubleTapLeft, "Double tap left"),
        (Self::DoubleTapRight, "Double tap right"),
        (Self::TwoFingerTapTopLeft, "Two-finger tap upper left"),
        (Self::TwoFingerTapTopRight, "Two-finger tap upper right"),
        (Self::TwoFingerTapBottomLeft, "Two-finger tap lower left"),
        (Self::TwoFingerTapBottomRight, "Two-finger tap lower right"),
        (Self::TwoFingerSwipeNorth, "Two-finger swipe north"),
        (Self::TwoFingerSwipeNorthEast, "Two-finger swipe north-east"),
        (Self::TwoFingerSwipeEast, "Two-finger swipe east"),
        (Self::TwoFingerSwipeSouthEast, "Two-finger swipe south-east"),
        (Self::TwoFingerSwipeSouth, "Two-finger swipe south"),
        (Self::TwoFingerSwipeSouthWest, "Two-finger swipe south-west"),
        (Self::TwoFingerSwipeWest, "Two-finger swipe west"),
        (Self::TwoFingerSwipeNorthWest, "Two-finger swipe north-west"),
        (Self::Spread, "Two-finger spread"),
        (Self::Pinch, "Two-finger pinch"),
        (Self::RotateClockwise, "Two-finger rotate clockwise"),
        (Self::RotateCounterclockwise, "Two-finger rotate counter-clockwise"),
    ];
    pub(super) fn label(self) -> &'static str {
        Self::ALL.iter().find(|entry| entry.0 == self).unwrap().1
    }

    fn touch(input: Input, viewport: ReadingViewport) -> Option<Self> {
        let (x, y, hold) = match input {
            Input::Tap(x, y) => (x, y, false),
            Input::LongTap(x, y) => (x, y, true),
            _ => return None,
        };
        let (width, height) = viewport.dimensions();
        let (width, height) = (width as f32, height as f32);
        let page_height = viewport.page_height() as f32;
        // Leave the visible footer, including Exit, available even if a corner is reassigned.
        if !(0.0..page_height).contains(&y) || !(0.0..width).contains(&x) {
            return None;
        }
        let right = x >= width * 0.875;
        let bottom = y >= height * 0.875;
        if (!right && x >= width / 8.0) || (!bottom && y >= height / 8.0) {
            return None;
        }
        Some(match (hold, bottom, right) {
            (false, false, false) => Self::TapTopLeft,
            (false, false, true) => Self::TapTopRight,
            (false, true, false) => Self::TapBottomLeft,
            (false, true, true) => Self::TapBottomRight,
            (true, false, false) => Self::HoldTopLeft,
            (true, false, true) => Self::HoldTopRight,
            (true, true, false) => Self::HoldBottomLeft,
            (true, true, true) => Self::HoldBottomRight,
        })
    }

    fn swipe(input: Input, x: f32, viewport: ReadingViewport) -> Option<Self> {
        let Input::Swipe { dx, dy, start_y: y } = input else {
            return None;
        };
        let (width, height) = viewport.dimensions();
        let (width, height) = (width as f32, height as f32);
        if !x.is_finite()
            || !y.is_finite()
            || !dx.is_finite()
            || !dy.is_finite()
            || !(0.0..width).contains(&x)
            || !(0.0..height).contains(&y)
        {
            return None;
        }
        let (ax, ay) = (dx.abs(), dy.abs());
        if ax.max(ay) < 72.0 {
            return None;
        }
        if ay > ax * 1.5 {
            // Top/bottom menu swipes retain priority at their shared corners.
            if !(height / 8.0..height * 0.875).contains(&y) {
                return None;
            }
            if x < 100.0 {
                return Some(if dy < 0.0 {
                    Self::LeftUp
                } else {
                    Self::LeftDown
                });
            }
            if x >= width - 100.0 {
                return Some(if dy < 0.0 {
                    Self::RightUp
                } else {
                    Self::RightDown
                });
            }
        } else if ax > ay * 1.5 {
            if y < 100.0 {
                return Some(if dx < 0.0 {
                    Self::TopLeft
                } else {
                    Self::TopRight
                });
            }
            if y >= height - 100.0 {
                return Some(if dx < 0.0 {
                    Self::BottomLeft
                } else {
                    Self::BottomRight
                });
            }
        } else if ax.hypot(ay) <= width / 4.0 {
            return Some(Self::ShortDiagonal);
        }
        None
    }

    fn multi_touch(input: MultiTouch, viewport: ReadingViewport) -> Option<Self> {
        match input {
            MultiTouch::TwoFingerTap { x, y, .. } => {
                let (width, height) = viewport.dimensions();
                let (width, height) = (width as f32, height as f32);
                if !(0.0..height).contains(&y) || !(0.0..width).contains(&x) {
                    return None;
                }
                let right = x >= width * 0.875;
                let bottom = y >= height * 0.875;
                if (!right && x >= width / 8.0) || (!bottom && y >= height / 8.0) {
                    return None;
                }
                Some(match (bottom, right) {
                    (false, false) => Self::TwoFingerTapTopLeft,
                    (false, true) => Self::TwoFingerTapTopRight,
                    (true, false) => Self::TwoFingerTapBottomLeft,
                    (true, true) => Self::TwoFingerTapBottomRight,
                })
            }
            MultiTouch::TwoFingerSwipe {
                dx,
                dy,
                start_x,
                start_y,
            } => {
                let (width, height) = viewport.dimensions();
                if !dx.is_finite()
                    || !dy.is_finite()
                    || !start_x.is_finite()
                    || !start_y.is_finite()
                    || !(0.0..width as f32).contains(&start_x)
                    || !(0.0..height as f32).contains(&start_y)
                {
                    return None;
                }
                let ax = dx.abs();
                let ay = dy.abs();
                // The shared input classifier already applies KOReader's per-contact pan
                // threshold; rechecking the midpoint here would reject shorter valid swipes.
                if ay > 0.577 * ax && ay < 1.732 * ax {
                    Some(match (dx > 0.0, dy > 0.0) {
                        (true, true) => Self::TwoFingerSwipeSouthEast,
                        (true, false) => Self::TwoFingerSwipeNorthEast,
                        (false, true) => Self::TwoFingerSwipeSouthWest,
                        (false, false) => Self::TwoFingerSwipeNorthWest,
                    })
                } else if ax > ay {
                    Some(if dx > 0.0 {
                        Self::TwoFingerSwipeEast
                    } else {
                        Self::TwoFingerSwipeWest
                    })
                } else {
                    Some(if dy > 0.0 {
                        Self::TwoFingerSwipeSouth
                    } else {
                        Self::TwoFingerSwipeNorth
                    })
                }
            }
            MultiTouch::Spread => Some(Self::Spread),
            MultiTouch::Pinch => Some(Self::Pinch),
            MultiTouch::RotateClockwise => Some(Self::RotateClockwise),
            MultiTouch::RotateCounterclockwise => Some(Self::RotateCounterclockwise),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Binding {
    Default,
    Ignore,
    PassThrough,
    Next,
    Previous,
    Contents,
    Bookmarks,
    Bookmark,
    Skim,
    Find,
    Dictionary,
    CycleDictionaryPresets,
    Highlights,
    GoTo,
    Back,
    Forward,
    NavigationBack,
    Refresh,
    Controls,
    PauseCornerHolds,
    LatestBookmark,
    NearestLink,
    FrontLight,
    RecentBooks,
    BookDetails,
    PreviousChapter,
    NextChapter,
    PreviousBook,
    FavoriteBooks,
    FitPageWidth,
    FitPageHeight,
    IncreaseFont,
    DecreaseFont,
    TogglePageScroll,
    PreviousTenPages,
    NextTenPages,
    BookShortcut(u64),
    /// Toggle one style tweak (KOReader's "toggle tweak" dispatcher action). The id is a preset id.
    StyleTweak(TweakId),
}

/// A preset id. A saved id must name a current preset, and maps back to the static one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TweakId(&'static str);

impl TweakId {
    pub(super) fn new(preset: &crate::user_styles::Preset) -> Self {
        Self(preset.id)
    }

    pub(super) fn id(self) -> &'static str {
        self.0
    }
}

impl Serialize for TweakId {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for TweakId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        crate::user_styles::preset(&id)
            .map(|preset| Self(preset.id))
            .ok_or_else(|| serde::de::Error::custom("unknown style tweak"))
    }
}

impl Binding {
    const ALL: [(Self, &'static str); 36] = [
        (Self::Default, "Default behavior"),
        (Self::Ignore, "Do nothing"),
        (Self::PassThrough, "Pass through"),
        (Self::Next, "Next page"),
        (Self::Previous, "Previous page"),
        (Self::Contents, "Contents"),
        (Self::Bookmarks, "Bookmarks"),
        (Self::Bookmark, "Toggle bookmark"),
        (Self::Skim, "Skim"),
        (Self::Find, "Find in book"),
        (Self::Dictionary, "Dictionary"),
        (
            Self::CycleDictionaryPresets,
            "Cycle through dictionary presets",
        ),
        (Self::Highlights, "Highlights"),
        (Self::GoTo, "Go to page"),
        (Self::Back, "Previous location"),
        (Self::Forward, "Next location"),
        (Self::NavigationBack, "Back (navigation settings)"),
        (Self::Refresh, "Refresh screen"),
        (Self::Controls, "Reading controls"),
        (Self::PauseCornerHolds, "Pause corner holds"),
        (Self::LatestBookmark, "Go to latest bookmark"),
        (Self::NearestLink, "Follow nearest link"),
        (Self::FrontLight, "Front light controls"),
        (Self::RecentBooks, "Recent books"),
        (Self::BookDetails, "Book details"),
        (Self::PreviousChapter, "Previous chapter"),
        (Self::NextChapter, "Next chapter"),
        (Self::PreviousBook, "Open previous book"),
        (Self::FavoriteBooks, "Favorite books"),
        (Self::FitPageWidth, "Fit page width"),
        (Self::FitPageHeight, "Fit page height"),
        (Self::IncreaseFont, "Increase font size"),
        (Self::DecreaseFont, "Decrease font size"),
        (Self::TogglePageScroll, "Toggle pages and scrolling"),
        (Self::PreviousTenPages, "Jump back 10 pages"),
        (Self::NextTenPages, "Jump forward 10 pages"),
    ];
    fn label(self, shortcuts: &book_shortcuts::State) -> String {
        if let Self::BookShortcut(id) = self {
            shortcuts.binding_label(id)
        } else if let Self::StyleTweak(id) = self {
            crate::user_styles::preset(id.id())
                .map_or(id.id(), |preset| preset.title)
                .into()
        } else {
            Self::ALL
                .iter()
                .find(|entry| entry.0 == self)
                .unwrap()
                .1
                .into()
        }
    }
}

pub(super) struct GestureBindings {
    path: PathBuf,
    bindings: BTreeMap<Gesture, Binding>,
    corner_pause_until: Option<Instant>,
    claimed_corner_hold: bool,
    replaying_hold_release: bool,
    pub(super) pending_double_tap: Option<PendingDoubleTap>,
    pub(super) replaying_double_tap: bool,
    pub(super) multiswipes: multiswipes::Multiswipes,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingDoubleTap {
    x: f32,
    y: f32,
    deadline: Instant,
    screen: Screen,
    book_id: Option<u64>,
    page: Option<usize>,
}

impl GestureBindings {
    pub fn load(data: &Path) -> Self {
        let path = data.join("reading-gestures.json");
        let bindings = read_import_file(&path, 8192)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            bindings,
            corner_pause_until: None,
            claimed_corner_hold: false,
            replaying_hold_release: false,
            pending_double_tap: None,
            replaying_double_tap: false,
            multiswipes: multiswipes::Multiswipes::load(data),
        }
    }
    fn get(&self, gesture: Gesture) -> Binding {
        self.bindings
            .get(&gesture)
            .copied()
            .unwrap_or(Binding::Default)
    }
    pub fn set(&mut self, gesture: Gesture, binding: Binding) -> Result<()> {
        let mut updated = self.bindings.clone();
        if binding == Binding::Default {
            updated.remove(&gesture);
        } else {
            updated.insert(gesture, binding);
        }
        primitive_app::save(&self.path, &serde_json::to_vec(&updated)?)?;
        self.bindings = updated;
        Ok(())
    }
    pub fn gestures(&self, shortcuts: &book_shortcuts::State) -> Vec<Item> {
        Gesture::ALL
            .iter()
            .map(|&(g, title)| {
                Item::new(title, Action::Show(Screen::GestureBinding(g)))
                    .detail(self.get(g).label(shortcuts))
            })
            .collect()
    }
    pub fn choices(&self, gesture: Gesture, shortcuts: &book_shortcuts::State) -> Vec<Item> {
        let current = self.get(gesture);
        let mut items: Vec<_> = Binding::ALL
            .iter()
            .filter(|(binding, _)| *binding != Binding::PassThrough)
            .map(|&(b, title)| {
                let item = Item::new(title, Action::BindGesture(gesture, b));
                if b == current {
                    item.detail("Selected")
                } else {
                    item
                }
            })
            .collect();
        items.extend(
            shortcuts.binding_items(current, |binding| Action::BindGesture(gesture, binding)),
        );
        // Every style tweak can be bound, after the fixed actions and the book shortcuts.
        items.extend(crate::user_styles::PRESETS.iter().map(|preset| {
            let binding = Binding::StyleTweak(TweakId::new(preset));
            let item = Item::new(preset.title, Action::BindGesture(gesture, binding));
            if binding == current {
                item.detail("Selected")
            } else {
                item
            }
        }));
        items
    }

    fn corner_holds_paused(&self, controls: &ReadingControls, now: Instant) -> bool {
        controls.ignore_corner_holds || self.corner_pause_until.is_some_and(|until| now < until)
    }

    fn pause_corner_holds(&mut self, controls: &ReadingControls, now: Instant) -> bool {
        if self.corner_holds_paused(controls, now) {
            return false;
        }
        self.corner_pause_until =
            Some(now + Duration::from_secs(u64::from(corner_pause_seconds(controls))));
        true
    }

    pub(super) fn consume_assigned_corner_hold(&mut self, input: Input) -> bool {
        if !self.claimed_corner_hold {
            return false;
        }
        match input {
            Input::HoldPan(..) => true,
            Input::HoldRelease { .. } => {
                self.claimed_corner_hold = false;
                true
            }
            Input::Sleep
            | Input::Wake
            | Input::HoldStart(..)
            | Input::Tap(..)
            | Input::LongTap(..) => {
                self.claimed_corner_hold = false;
                false
            }
            _ => false,
        }
    }

    pub(super) fn clear_assigned_corner_hold(&mut self) {
        self.claimed_corner_hold = false;
    }

    pub(super) fn reserves_swipe(
        &self,
        input: Input,
        start_x: f32,
        viewport: ReadingViewport,
    ) -> bool {
        Gesture::swipe(input, start_x, viewport).is_some_and(|gesture| {
            self.get(gesture) != Binding::Default
                || matches!(
                    gesture,
                    Gesture::LeftUp
                        | Gesture::LeftDown
                        | Gesture::RightUp
                        | Gesture::RightDown
                        | Gesture::ShortDiagonal
                )
        })
    }
}

impl Reader {
    pub(super) fn multi_touch_input(&mut self, input: MultiTouch) -> Change {
        let pending_change = self.gestures.pending_double_tap.take().and_then(|pending| {
            self.double_tap_context_matches(pending)
                .then(|| self.replay_double_tap(pending))
        });
        if let MultiTouch::TwoFingerTap {
            held_ms,
            fallback_x,
            fallback_y,
            ..
        } = input
        {
            if held_ms >= TWO_FINGER_HOLD_INTERVAL_MS {
                return pending_change.unwrap_or(Change::None);
            }
            if held_ms >= u32::from(self.controls.two_finger_tap_duration_ms()) {
                let fallback = self.input(Input::Tap(fallback_x, fallback_y));
                return pending_change.map_or(fallback, |pending| merge_changes(pending, fallback));
            }
        }
        if !self.reading_overlays_clear()
            || self.touch_locked
            || self.selection.is_some()
            || self.pdf_selection.is_some()
        {
            return pending_change.unwrap_or(Change::None);
        }
        let Some(gesture) = Gesture::multi_touch(input, self.reading_viewport()) else {
            return pending_change.unwrap_or(Change::None);
        };
        self.gestures.multiswipes.clear_contact();
        self.finish_interrupted_scroll();
        self.auto_turn.activity(Instant::now());
        let binding = self.gestures.get(gesture);
        let change = if binding == Binding::Default {
            match gesture {
                Gesture::TwoFingerSwipeEast => self
                    .dispatch_gesture_binding(Binding::Contents, None)
                    .unwrap_or(Change::None),
                Gesture::TwoFingerSwipeWest => self
                    .dispatch_gesture_binding(Binding::Bookmarks, None)
                    .unwrap_or(Change::None),
                Gesture::TwoFingerSwipeNorth | Gesture::TwoFingerSwipeSouth => {
                    let MultiTouch::TwoFingerSwipe { dy, .. } = input else {
                        return pending_change.unwrap_or(Change::None);
                    };
                    #[cfg(all(target_os = "linux", target_arch = "arm"))]
                    {
                        self.quick_display = super::QuickDisplay::read();
                    }
                    let level = DisplayLevel::FrontLight;
                    if self.quick_display.level(level).is_none() {
                        Change::None
                    } else {
                        self.auto_turn.activity(Instant::now());
                        match self.quick_display.swipe(level, -dy) {
                            Ok(()) => Change::None,
                            Err(error) => self.report(error),
                        }
                    }
                }
                Gesture::Spread => self
                    .dispatch_gesture_binding(Binding::IncreaseFont, None)
                    .unwrap_or(Change::None),
                Gesture::Pinch => self
                    .dispatch_gesture_binding(Binding::DecreaseFont, None)
                    .unwrap_or(Change::None),
                _ => Change::None,
            }
        } else {
            let location = match input {
                MultiTouch::TwoFingerTap { x, y, .. } => Some((x, y)),
                MultiTouch::TwoFingerSwipe {
                    start_x, start_y, ..
                } => Some((start_x, start_y)),
                _ => None,
            };
            self.dispatch_gesture_binding(binding, location)
                .unwrap_or(Change::None)
        };
        pending_change.map_or(change, |pending| merge_changes(pending, change))
    }

    pub(super) fn double_tap_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            self.gestures.pending_double_tap = None;
            return None;
        }

        let now = Instant::now();
        let current_tap = match input {
            Input::Tap(x, y) if self.double_tap_eligible(x, y) => Some((x, y)),
            _ => None,
        };

        if let Some(pending) = self.gestures.pending_double_tap.take() {
            if !self.double_tap_context_matches(pending) {
                if let Some((x, y)) = current_tap {
                    self.gestures.pending_double_tap = Some(self.pending_double_tap(x, y, now));
                    return Some(Change::None);
                }
                return None;
            }
            if let Some((x, y)) = current_tap
                && now <= pending.deadline
                && (x - pending.x).hypot(y - pending.y) <= DOUBLE_TAP_DISTANCE_PX
            {
                return Some(self.dispatch_double_tap(x, y));
            }

            let previous = self.replay_double_tap(pending);
            if let Some((x, y)) = current_tap
                && self.double_tap_eligible(x, y)
            {
                self.gestures.pending_double_tap = Some(self.pending_double_tap(x, y, now));
                return Some(previous);
            }

            self.gestures.replaying_double_tap = true;
            let current = self.input(input);
            self.gestures.replaying_double_tap = false;
            return Some(merge_changes(previous, current));
        }

        let (x, y) = current_tap?;
        self.gestures.pending_double_tap = Some(self.pending_double_tap(x, y, now));
        Some(Change::None)
    }

    pub(super) fn double_tap_deadline(&self) -> Option<Instant> {
        self.gestures
            .pending_double_tap
            .map(|pending| pending.deadline)
    }

    pub(super) fn finish_double_tap(&mut self, now: Instant) -> Option<Change> {
        let pending = self
            .gestures
            .pending_double_tap
            .filter(|pending| pending.deadline <= now)?;
        self.gestures.pending_double_tap = None;
        Some(
            if self.double_tap_context_matches(pending)
                && self.double_tap_eligible(pending.x, pending.y)
            {
                self.replay_double_tap(pending)
            } else {
                Change::None
            },
        )
    }

    fn pending_double_tap(&self, x: f32, y: f32, now: Instant) -> PendingDoubleTap {
        PendingDoubleTap {
            x,
            y,
            deadline: now
                + Duration::from_millis(u64::from(self.controls.double_tap_interval_ms())),
            screen: self.screen,
            book_id: self.open_book_id(),
            page: self
                .active
                .as_ref()
                .map(|book| book.reading.page)
                .or_else(|| self.fixed.as_ref().map(|book| book.reading.page())),
        }
    }

    fn double_tap_context_matches(&self, pending: PendingDoubleTap) -> bool {
        self.screen == pending.screen
            && self.open_book_id() == pending.book_id
            && self
                .active
                .as_ref()
                .map(|book| book.reading.page)
                .or_else(|| self.fixed.as_ref().map(|book| book.reading.page()))
                == pending.page
    }

    fn double_tap_eligible(&self, x: f32, y: f32) -> bool {
        if !self.controls.double_tap_page_jump
            || !self.reading_overlays_clear()
            || self.touch_locked
            || self.selection.is_some()
            || self.pdf_selection.is_some()
            || !x.is_finite()
            || !y.is_finite()
            || self.saved_mark_at(x, y)
        {
            return false;
        }
        let viewport = self.reading_viewport();
        let (width, _) = viewport.dimensions();
        if !(0.0..width as f32).contains(&x) || !(0.0..viewport.page_height() as f32).contains(&y) {
            return false;
        }
        Gesture::touch(Input::Tap(x, y), viewport)
            .is_none_or(|gesture| self.gestures.get(gesture) == Binding::Default)
    }

    fn replay_double_tap(&mut self, pending: PendingDoubleTap) -> Change {
        self.gestures.replaying_double_tap = true;
        let change = self.input(Input::Tap(pending.x, pending.y));
        self.gestures.replaying_double_tap = false;
        change
    }

    fn dispatch_double_tap(&mut self, x: f32, y: f32) -> Change {
        let left = x < self.reading_viewport().dimensions().0 as f32 / 2.0;
        let gesture = if left {
            Gesture::DoubleTapLeft
        } else {
            Gesture::DoubleTapRight
        };
        let binding = self.gestures.get(gesture);
        if binding == Binding::Default {
            return self.change_page_by_relative(if left { -10 } else { 10 });
        }
        self.dispatch_gesture_binding(binding, Some((x, y)))
            .unwrap_or(Change::None)
    }

    pub(super) fn reading_gesture_items(&self) -> Vec<Item> {
        let multiswipes_enabled = self.gestures.multiswipes.enabled();
        let toggle = Action::Multiswipe(multiswipes::Command::Toggle);
        let submenu = |screen| {
            if multiswipes_enabled {
                Action::Show(screen)
            } else {
                Action::Heading
            }
        };
        let mut items = vec![
            Item::new(
                "Double tap page jump",
                Action::SetReadingControls(ReadingControls {
                    double_tap_page_jump: !self.controls.double_tap_page_jump,
                    ..self.controls
                }),
            )
            .detail(if self.controls.double_tap_page_jump {
                "On"
            } else {
                "Off"
            }),
            Item::new(
                "Double-tap interval",
                Action::Show(Screen::NumberingValue(
                    super::NumberField::DoubleTapInterval,
                )),
            )
            .detail(format!("{} ms", self.controls.double_tap_interval_ms())),
            Item::new(
                "Two-finger tap duration",
                Action::Show(Screen::NumberingValue(
                    super::NumberField::TwoFingerTapDuration,
                )),
            )
            .detail(format!(
                "{} ms",
                self.controls.two_finger_tap_duration_ms()
            )),
            Item::new("Multiswipes", toggle).detail("Draw a shape with one finger"),
            Item::new("Preset multiswipes", submenu(Screen::Multiswipes)),
            Item::new("Custom multiswipes", submenu(Screen::CustomMultiswipes)),
            Item::new("Gesture overview", Action::Show(Screen::GestureOverview)),
        ];
        items.extend(self.gestures.gestures(&self.book_shortcuts));
        items.extend([
            Item::new(
                "Ignore long-press on corners",
                Action::SetReadingControls(ReadingControls {
                    ignore_corner_holds: !self.controls.ignore_corner_holds,
                    ..self.controls
                }),
            ),
            Item::new(
                "Corner hold pause",
                Action::Show(Screen::NumberingValue(super::NumberField::CornerHoldPause)),
            )
            .detail(format!("{} seconds", corner_pause_seconds(&self.controls))),
            self.scroll_settings_item(),
        ]);
        items
    }

    pub(super) fn return_to_gesture_list(&mut self, gesture: Gesture) {
        self.show(Screen::Gestures);
        if let Some(index) = self.reading_gesture_items().iter().position(
            |item| matches!(item.action, Action::Show(Screen::GestureBinding(item_gesture)) if item_gesture == gesture),
        ) {
            self.list_page = index / self.list_geometry().rows;
        }
    }

    pub(super) fn start_assigned_corner_hold(&mut self, x: f32, y: f32) -> Option<Change> {
        let change = self.assigned_gesture(Input::LongTap(x, y), None)?;
        self.touch_hold = None;
        self.gestures.claimed_corner_hold = true;
        Some(change)
    }

    pub(super) fn unassigned_hold_release(&mut self, x: f32, y: f32) -> Change {
        // A physical hold was already routed at its contact origin. In
        // particular, an expired timed pause must not claim its release.
        self.gestures.replaying_hold_release = true;
        let change = self.input(Input::LongTap(x, y));
        self.gestures.replaying_hold_release = false;
        change
    }

    pub(super) fn assigned_gesture(
        &mut self,
        input: Input,
        swipe_x: Option<f32>,
    ) -> Option<Change> {
        if self.touch_locked
            || !matches!(self.screen, Screen::Book | Screen::Fixed | Screen::Library)
            || self.selection.is_some()
            || self.pdf_selection.is_some()
        {
            return None;
        }
        if matches!(input, Input::LongTap(..))
            && (self.gestures.replaying_hold_release
                || self
                    .gestures
                    .corner_holds_paused(&self.controls, Instant::now())
                || (self.screen != Screen::Library && !self.touch_selection_enabled()))
        {
            return None;
        }
        if self.screen == Screen::Library
            && matches!(input, Input::Tap(x, y) | Input::LongTap(x, y) if self.navigation_layout().back().contains(x, y))
        {
            return None;
        }
        let viewport = self.reading_viewport();
        let gesture = match swipe_x {
            Some(x) => Gesture::swipe(input, x, viewport)?,
            None => Gesture::touch(input, viewport)?,
        };
        let binding = self.gestures.get(gesture);
        if self.screen == Screen::Library
            && !matches!(
                binding,
                Binding::NavigationBack | Binding::BookShortcut(_) | Binding::StyleTweak(_)
            )
        {
            return None;
        }
        if let (Binding::Default, Input::Swipe { dy, .. }) = (binding, input) {
            let level = match gesture {
                Gesture::LeftUp | Gesture::LeftDown => Some(DisplayLevel::FrontLight),
                Gesture::RightUp | Gesture::RightDown => Some(DisplayLevel::Warmth),
                _ => None,
            };
            if let Some(level) = level {
                #[cfg(all(target_os = "linux", target_arch = "arm"))]
                {
                    self.quick_display = super::QuickDisplay::read();
                }
                self.quick_display.level(level)?;
                self.auto_turn.activity(Instant::now());
                return Some(match self.quick_display.swipe(level, -dy) {
                    Ok(()) => Change::None,
                    Err(error) => self.report(error),
                });
            }
        }
        if binding == Binding::Default {
            if gesture == Gesture::ShortDiagonal {
                self.auto_turn.activity(Instant::now());
                self.pages_since_refresh = 0;
                return Some(Change::Full);
            }
            return None;
        }
        let location = match input {
            Input::Tap(x, y) | Input::LongTap(x, y) => Some((x, y)),
            Input::Swipe { start_y, .. } => swipe_x.map(|x| (x, start_y)),
            _ => None,
        };
        self.dispatch_gesture_binding(binding, location)
    }

    fn dispatch_gesture_binding(
        &mut self,
        binding: Binding,
        location: Option<(f32, f32)>,
    ) -> Option<Change> {
        self.auto_turn.activity(Instant::now());
        let fixed = self.fixed.is_some();
        let action = match binding {
            Binding::Default | Binding::PassThrough => return None,
            Binding::Ignore => return Some(Change::None),
            Binding::Next => return Some(self.input(Input::PageForward)),
            Binding::Previous => return Some(self.input(Input::PageBack)),
            Binding::PreviousTenPages => return Some(self.change_page_by_relative(-10)),
            Binding::NextTenPages => return Some(self.change_page_by_relative(10)),
            Binding::Controls => {
                let x = self.reading_viewport().dimensions().0 as f32 / 2.0;
                return Some(self.open_reading_edge(true, x));
            }
            Binding::PauseCornerHolds => {
                if !self
                    .gestures
                    .pause_corner_holds(&self.controls, Instant::now())
                {
                    return Some(Change::None);
                }
                self.notice = Some(format!(
                    "Corner holds paused for {} seconds",
                    corner_pause_seconds(&self.controls)
                ));
                return Some(Change::Partial);
            }
            Binding::Contents => Action::Show(if fixed {
                Screen::FixedContents
            } else {
                Screen::Contents
            }),
            Binding::Bookmarks => Action::Show(if fixed {
                Screen::FixedBookmarks
            } else {
                Screen::Bookmarks
            }),
            Binding::Bookmark => Action::ToggleBookmark,
            Binding::Skim => Action::Skim,
            Binding::Find => Action::Show(Screen::Find),
            Binding::Dictionary => Action::Show(Screen::DictionaryEntry),
            Binding::CycleDictionaryPresets => {
                Action::DictionaryPreset(super::dictionary_presets::Command::Cycle)
            }
            Binding::Highlights => Action::Show(Screen::Highlights),
            Binding::GoTo => Action::Show(Screen::GoToPage),
            Binding::Back => Action::ReturnToLocation,
            Binding::Forward => Action::NextLocation,
            Binding::Refresh => Action::RefreshDisplay,
            Binding::NavigationBack => return Some(self.navigation_back()),
            Binding::LatestBookmark => return self.jump_to_latest_bookmark(),
            Binding::NearestLink => {
                let (x, y) = location?;
                return self.follow_nearest_link_at(x, y);
            }
            Binding::FrontLight => {
                let x = self.reading_viewport().dimensions().0 as f32 / 2.0;
                let change = self.open_reading_edge(true, x);
                self.top_group = Some(TopGroup::Settings);
                return Some(change);
            }
            Binding::RecentBooks => Action::ListBooks(BookList::Recent),
            Binding::BookDetails => Action::OrganizeCurrent,
            Binding::PreviousChapter => {
                Action::Chapter(super::chapter_navigation::Direction::Previous)
            }
            Binding::NextChapter => Action::Chapter(super::chapter_navigation::Direction::Next),
            Binding::PreviousBook => Action::OpenPreviousDocument,
            Binding::FavoriteBooks => Action::ListBooks(BookList::Favorites),
            Binding::FitPageWidth if fixed => Action::FitPage(true),
            Binding::FitPageHeight if fixed => Action::FitHeight,
            Binding::FitPageWidth | Binding::FitPageHeight => return None,
            Binding::IncreaseFont | Binding::DecreaseFont => {
                if fixed {
                    return Some(self.activate(Action::AdjustFixedZoomFactor(if binding
                        == Binding::IncreaseFont
                    {
                        10
                    } else {
                        -10
                    }))
                    .map(|()| self.applied_change())
                    .unwrap_or_else(|error| self.report(error)));
                }
                let Some(active) = self.active.as_ref() else {
                    return Some(Change::None);
                };
                let owner = active.id;
                let [larger, smaller, _] = super::text_size_items(
                    active.reading.config.settings,
                    owner,
                    self.pending_logical_size(owner),
                );
                if binding == Binding::IncreaseFont {
                    larger.action
                } else {
                    smaller.action
                }
            }
            Binding::TogglePageScroll => {
                let Some(active) = self.active.as_ref() else {
                    return Some(Change::None);
                };
                if active.reading.probe.is_vertical() {
                    return Some(Change::None);
                }
                Action::ReadingMode(!active.reading.config.scroll)
            }
            Binding::BookShortcut(id) => Action::BookShortcut(book_shortcuts::Command::Launch(id)),
            Binding::StyleTweak(id) => {
                Action::Style(super::style_tweaks::Command::Preset(id.id().into()))
            }
        };
        Some(match self.activate(action) {
            Ok(()) => {
                let message = match binding {
                    Binding::Back => Some("Returned to previous location"),
                    Binding::Forward => Some("Returned to next location"),
                    _ => None,
                };
                if let Some(message) = message {
                    self.notify_reading(super::reading_notifications::Source::Dispatcher, message);
                }
                self.applied_change()
            }
            Err(error) => self.report(error),
        })
    }
}

fn merge_changes(first: Change, second: Change) -> Change {
    match (first, second) {
        (Change::Exit, _) | (_, Change::Exit) => Change::Exit,
        (Change::Full, _) | (_, Change::Full) => Change::Full,
        (Change::Partial, _) | (_, Change::Partial) => Change::Partial,
        _ => Change::None,
    }
}

#[cfg(test)]
mod tests;
