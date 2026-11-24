//! Assign single-contact gestures to existing Reader actions. The SDK owns system gestures.
use super::{Action, DisplayLevel, Item, Reader, Screen, read_import_file};
use crate::{Result, reading_controls::ReadingControls, session::ReadingViewport};
use primitive_app::{App, Change};
use primitive_ui::Input;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) const CORNER_PAUSE_RANGE: std::ops::RangeInclusive<u8> = 5..=30;

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
}

impl Gesture {
    pub(super) const ALL: [(Self, &'static str); 17] = [
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Binding {
    Default,
    Ignore,
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
}

impl Binding {
    const ALL: [(Self, &'static str); 19] = [
        (Self::Default, "Default behavior"),
        (Self::Ignore, "Do nothing"),
        (Self::Next, "Next page"),
        (Self::Previous, "Previous page"),
        (Self::Contents, "Contents"),
        (Self::Bookmarks, "Bookmarks"),
        (Self::Bookmark, "Toggle bookmark"),
        (Self::Skim, "Page browser"),
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
    ];
    fn label(self) -> &'static str {
        Self::ALL.iter().find(|entry| entry.0 == self).unwrap().1
    }
}

pub(super) struct GestureBindings {
    path: PathBuf,
    bindings: BTreeMap<Gesture, Binding>,
    corner_pause_until: Option<Instant>,
    claimed_corner_hold: bool,
    replaying_hold_release: bool,
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
    pub fn gestures(&self) -> Vec<Item> {
        Gesture::ALL
            .iter()
            .map(|&(g, title)| {
                Item::new(title, Action::Show(Screen::GestureBinding(g)))
                    .detail(self.get(g).label())
            })
            .collect()
    }
    pub fn choices(&self, gesture: Gesture) -> Vec<Item> {
        let current = self.get(gesture);
        Binding::ALL
            .iter()
            .map(|&(b, title)| {
                let item = Item::new(title, Action::BindGesture(gesture, b));
                if b == current {
                    item.detail("Selected")
                } else {
                    item
                }
            })
            .collect()
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
}

impl Reader {
    pub(super) fn reading_gesture_items(&self) -> Vec<Item> {
        let mut items = self.gestures.gestures();
        items.extend([
            Item::new(
                "Ignore long-press on corners",
                Action::SetReadingControls(ReadingControls {
                    ignore_corner_holds: !self.controls.ignore_corner_holds,
                    ..self.controls
                }),
            )
            .detail(if self.controls.ignore_corner_holds {
                "On"
            } else {
                "Off"
            }),
            Item::new(
                "Corner hold pause",
                Action::Show(Screen::NumberingValue(super::NumberField::CornerHoldPause)),
            )
            .detail(format!("{} seconds", corner_pause_seconds(&self.controls))),
        ]);
        items
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
        if binding == Binding::NavigationBack {
            return Some(self.navigation_back());
        }
        if self.screen == Screen::Library {
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
        self.auto_turn.activity(Instant::now());
        let fixed = self.fixed.is_some();
        let action = match binding {
            Binding::Default => return None,
            Binding::Ignore => return Some(Change::None),
            Binding::Next => return Some(self.input(Input::PageForward)),
            Binding::Previous => return Some(self.input(Input::PageBack)),
            Binding::Controls => {
                return Some(self.open_reading_edge(true, viewport.dimensions().0 as f32 / 2.0));
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
            Binding::NavigationBack => unreachable!("Back is handled before reading-only actions"),
        };
        Some(match self.activate(action) {
            Ok(()) => self.applied_change(),
            Err(error) => self.report(error),
        })
    }
}

#[cfg(test)]
mod tests;
