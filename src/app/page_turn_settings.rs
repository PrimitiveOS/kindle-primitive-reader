//! Document touch navigation stays separate from physical page buttons and the document layout.
use super::step_buttons::StepButtons;
use super::{Action, Canvas, Change, Input, Item, Reader, Rect, Screen};
use crate::{Result, session::ReadingViewport};
use primitive_ui::{CAPTION, RowAccessory, TextStyle};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Zones {
    #[default]
    Default,
    LeftRight,
    TopBottom,
    BottomTop,
}

impl Zones {
    const ALL: [Self; 4] = [
        Self::Default,
        Self::LeftRight,
        Self::TopBottom,
        Self::BottomTop,
    ];

    pub(crate) fn is_default(&self) -> bool {
        *self == Self::Default
    }

    fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::LeftRight => "Left / right",
            Self::TopBottom => "Top / bottom",
            Self::BottomTop => "Bottom / top",
        }
    }

    fn measure(self) -> &'static str {
        match self {
            Self::Default | Self::LeftRight => "screen width",
            Self::TopBottom | Self::BottomTop => "screen height",
        }
    }

    fn tap(
        self,
        viewport: ReadingViewport,
        body_end: u32,
        sizes: Sizes,
        invert: bool,
        x: f32,
        y: f32,
    ) -> Option<Input> {
        let (width, height) = viewport.dimensions();
        let (width, height) = (width as f32, height as f32);
        if !(0.0..width).contains(&x) || !(0.0..body_end as f32).contains(&y) {
            return None;
        }
        let (coordinate, extent, forward_first) = match self {
            Self::Default | Self::LeftRight => (x, width, invert),
            Self::TopBottom => (y, height, false),
            Self::BottomTop => (y, height, true),
        };
        let sizes = if self == Self::Default {
            Sizes::default()
        } else {
            sizes
        };
        let (leading, trailing) = if forward_first {
            (sizes.forward, sizes.backward)
        } else {
            (sizes.backward, sizes.forward)
        };
        if coordinate < extent * f32::from(leading) / 100.0 {
            Some(if forward_first {
                Input::PageForward
            } else {
                Input::PageBack
            })
        } else if coordinate >= extent * (100.0 - f32::from(trailing)) / 100.0 {
            Some(if forward_first {
                Input::PageBack
            } else {
                Input::PageForward
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Sizes {
    backward: u8,
    forward: u8,
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            backward: 25,
            forward: 75,
        }
    }
}

impl Sizes {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn normalize(self) -> Self {
        let forward = self.forward.min(100);
        Self {
            backward: self.backward.min(100 - forward),
            forward,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Order {
    #[default]
    Publication,
    LeftToRight,
    RightToLeft,
}

impl Order {
    const ALL: [Self; 3] = [Self::Publication, Self::LeftToRight, Self::RightToLeft];

    pub(crate) fn is_default(&self) -> bool {
        *self == Self::Publication
    }

    fn label(self) -> &'static str {
        match self {
            Self::Publication => "Follow the publication",
            Self::LeftToRight => "Left to right",
            Self::RightToLeft => "Right to left",
        }
    }

    fn reverse(self, publication_rtl: bool, legacy_mirror: bool) -> bool {
        match self {
            Self::Publication => publication_rtl ^ legacy_mirror,
            Self::LeftToRight => false,
            Self::RightToLeft => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Document {
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<Order>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mirror_dialogs: Option<bool>,
}

impl Document {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn fill_missing(&mut self, defaults: Self) {
        self.order = self.order.or(defaults.order);
        self.mirror_dialogs = self.mirror_dialogs.or(defaults.mirror_dialogs);
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Command {
    Taps,
    Swipes,
    SwipesWhileLocked,
    ChooseZones(Zones),
    Order(Order),
    MirrorDialogs,
    DefaultOrder(Order),
    DefaultMirrorDialogs,
    ResetLegacyTaps,
}

impl Reader {
    pub(super) fn default_document_turns(&self) -> Document {
        Document {
            order: Some(self.controls.touch_turn_order),
            mirror_dialogs: Some(self.controls.mirror_document_dialogs),
        }
    }

    fn document_turns(&self) -> Document {
        let mut turns = self
            .open_book_id()
            .and_then(|id| self.library.state(id))
            .map_or_else(Document::default, |state| state.document_turns);
        turns.fill_missing(self.default_document_turns());
        turns
    }

    pub(super) fn document_dialogs_mirrored(&self) -> bool {
        self.open_book_id().is_some() && self.document_turns().mirror_dialogs == Some(true)
    }

    fn authored_rtl(&self) -> bool {
        self.fixed.as_ref().map_or_else(
            || {
                self.active
                    .as_ref()
                    .is_some_and(|book| book.reading.probe.pages_right_to_left())
            },
            |book| book.reading.pages_right_to_left(),
        )
    }

    pub(super) fn touch_turn_inverted(&self, is_tap: bool) -> bool {
        self.document_turns()
            .order
            .unwrap_or_default()
            .reverse(self.authored_rtl(), is_tap && self.controls.left_touch)
    }

    pub(super) fn tap_turn(&self, x: f32, y: f32, body_end: u32) -> Option<Input> {
        if self.controls.disable_page_taps {
            return None;
        }
        self.controls.page_turn_zones.tap(
            self.reading_viewport(),
            body_end,
            self.controls.page_turn_sizes,
            self.touch_turn_inverted(true),
            x,
            y,
        )
    }

    pub(super) fn swipe_turn(&self, dx: f32, dy: f32) -> Option<Input> {
        if self.controls.disable_page_swipes
            || !dx.is_finite()
            || !dy.is_finite()
            || dx == 0.0
            || dy.abs() > dx.abs() * 0.577
        {
            return None;
        }
        Some(if (dx < 0.0) ^ self.touch_turn_inverted(false) {
            Input::PageForward
        } else {
            Input::PageBack
        })
    }

    pub(super) fn locked_swipe_turn(&self, input: Input) -> Option<Input> {
        let Input::Swipe { dx, dy, .. } = input else {
            return None;
        };
        if !self.touch_locked
            || !self.controls.page_swipes_while_locked
            || !matches!(self.screen, Screen::Book | Screen::Fixed)
            || self.selection.is_some()
            || self.pdf_selection.is_some()
        {
            return None;
        }
        self.swipe_turn(dx, dy)
    }

    pub(super) fn page_turn_items(&self, screen: Screen) -> Vec<Item> {
        let language = self.controls.interface_language;
        let item = |text: &str, command| {
            Item::new(
                language.page_turn_settings_text(text),
                Action::PageTurnSetting(command),
            )
        };
        match screen {
            Screen::PageTurnZones => {
                let mut items = Zones::ALL
                    .into_iter()
                    .map(|zones| item(zones.label(), Command::ChooseZones(zones)))
                    .collect::<Vec<_>>();
                if self.controls.page_turn_zones != Zones::Default {
                    let sizes = self.controls.page_turn_sizes;
                    items.push(
                        Item::new(
                            language.page_turn_settings_text("Backward and forward zone sizes"),
                            Action::Show(Screen::PageTurnSizes(sizes)),
                        )
                        .detail(format!("{}% / {}%", sizes.backward, sizes.forward)),
                    );
                }
                items
            }
            Screen::PageTurnDefaults => {
                let mut items = Order::ALL
                    .into_iter()
                    .map(|order| item(order.label(), Command::DefaultOrder(order)))
                    .collect::<Vec<_>>();
                items.push(item(
                    "Invert document-related dialogs",
                    Command::DefaultMirrorDialogs,
                ));
                if self.controls.left_touch {
                    items.push(item("Mirror taps only", Command::ResetLegacyTaps).detail(
                        language.page_turn_settings_text("Retained from an earlier version"),
                    ));
                }
                items
            }
            _ => {
                let mut items = vec![
                    item("With taps", Command::Taps),
                    item("With swipes", Command::Swipes),
                ];
                items.push(if self.controls.disable_page_swipes {
                    Item::new(
                        language.page_turn_settings_text("Swipes always active"),
                        Action::Heading,
                    )
                    .detail(
                        language
                            .page_turn_settings_text("Enable page swipes to change this setting"),
                    )
                } else {
                    item("Swipes always active", Command::SwipesWhileLocked).detail(
                        language.page_turn_settings_text(
                            "Turn pages with horizontal swipes while touch is locked",
                        ),
                    )
                });
                items.push(if self.controls.disable_page_taps {
                    Item::new(
                        language.page_turn_settings_text("Tap zones"),
                        Action::Heading,
                    )
                    .detail(language.page_turn_settings_text("Enable page taps to choose a layout"))
                } else {
                    Item::new(
                        language.page_turn_settings_text("Tap zones"),
                        Action::Show(Screen::PageTurnZones),
                    )
                    .detail(language.page_turn_settings_text(self.controls.page_turn_zones.label()))
                });
                if self.open_book_id().is_some() {
                    let direction = if self.touch_turn_inverted(false) {
                        "Right to left"
                    } else {
                        "Left to right"
                    };
                    let next = if self.touch_turn_inverted(false) {
                        Order::LeftToRight
                    } else {
                        Order::RightToLeft
                    };
                    items.extend([
                        item("Invert page turn taps and swipes", Command::Order(next))
                            .detail(language.page_turn_settings_text(direction)),
                        item("Invert document-related dialogs", Command::MirrorDialogs).detail(
                            language.page_turn_settings_text("Contents, Book Map and Page Browser"),
                        ),
                    ]);
                }
                items.push(Item::new(
                    language.page_turn_settings_text("Defaults for new books"),
                    Action::Show(Screen::PageTurnDefaults),
                ));
                items
            }
        }
    }

    pub(super) fn page_turn_accessory(&self, command: Command) -> RowAccessory {
        RowAccessory::Toggle(match command {
            Command::Taps => !self.controls.disable_page_taps,
            Command::Swipes => !self.controls.disable_page_swipes,
            Command::SwipesWhileLocked => self.controls.page_swipes_while_locked,
            Command::ChooseZones(zones) => self.controls.page_turn_zones == zones,
            Command::Order(_) => self.touch_turn_inverted(false),
            Command::MirrorDialogs => self.document_dialogs_mirrored(),
            Command::DefaultOrder(order) => self.controls.touch_turn_order == order,
            Command::DefaultMirrorDialogs => self.controls.mirror_document_dialogs,
            Command::ResetLegacyTaps => self.controls.left_touch,
        })
    }

    pub(super) fn change_page_turn_setting(&mut self, command: Command) -> Result<()> {
        let mut controls = self.controls;
        match command {
            Command::Taps => controls.disable_page_taps = !controls.disable_page_taps,
            Command::Swipes => controls.disable_page_swipes = !controls.disable_page_swipes,
            Command::SwipesWhileLocked => {
                controls.page_swipes_while_locked = !controls.page_swipes_while_locked
            }
            Command::ChooseZones(zones) => controls.page_turn_zones = zones,
            Command::DefaultOrder(order) => controls.touch_turn_order = order,
            Command::ResetLegacyTaps => controls.left_touch = false,
            Command::DefaultMirrorDialogs => {
                controls.mirror_document_dialogs = !controls.mirror_document_dialogs
            }
            Command::Order(_) | Command::MirrorDialogs => {
                let id = self.open_book_id().ok_or("Open a book first")?;
                let mut turns = self.document_turns();
                match command {
                    Command::Order(order) => turns.order = Some(order),
                    Command::MirrorDialogs => {
                        turns.mirror_dialogs = Some(!self.document_dialogs_mirrored())
                    }
                    _ => unreachable!(),
                }
                let state = self
                    .library
                    .state_mut(id)
                    .ok_or("Reading state unavailable")?;
                let before = std::mem::replace(&mut state.document_turns, turns);
                let was_dirty = self.dirty;
                self.dirty = true;
                if let Err(error) = self.save_library() {
                    self.library.state_mut(id).unwrap().document_turns = before;
                    self.dirty = was_dirty;
                    return Err(error.into());
                }
                return Ok(());
            }
        }
        self.activate(Action::SetReadingControls(controls))
    }

    pub(super) fn draw_page_turn_sizes(&self, canvas: &mut Canvas, sizes: Sizes) {
        let navigation = self.navigation_layout();
        let width = navigation.width;
        let detail = format!(
            "Percentage of {}. Uncovered space will not turn a page.",
            self.controls.page_turn_zones.measure()
        );
        canvas.text(
            Rect::new(48, 196, width - 96, 80),
            &detail,
            CAPTION.lines(2),
        );
        for (index, (label, size)) in [("Backward", sizes.backward), ("Forward", sizes.forward)]
            .into_iter()
            .enumerate()
        {
            let (body, buttons) = zone_size_controls(width, index);
            canvas.text(
                body,
                &format!("{label}     {size}%"),
                TextStyle::new(44).bold(),
            );
            buttons.draw(canvas);
        }
        let (defaults, apply) = zone_size_actions(width);
        canvas.button(defaults, "Reset", false, false);
        canvas.button(apply, "Apply", true, false);
        if let Some(notice) = &self.notice {
            canvas.text(Rect::new(48, 776, width - 96, 80), notice, CAPTION.lines(2));
        }
    }

    pub(super) fn page_turn_sizes_input(&mut self, input: Input, mut sizes: Sizes) -> Change {
        let navigation = self.navigation_layout();
        if input == Input::PageBack
            || matches!(input, Input::Tap(x, y) if navigation.back().contains(x, y))
        {
            self.show(Screen::PageTurnZones);
            return Change::Partial;
        }
        let (defaults, apply) = zone_size_actions(navigation.width);
        if matches!(input, Input::Tap(x, y) if apply.contains(x, y)) {
            let controls = crate::reading_controls::ReadingControls {
                page_turn_sizes: sizes.normalize(),
                ..self.controls
            };
            return match self.activate(Action::SetReadingControls(controls)) {
                Ok(()) => {
                    self.show(Screen::PageTurnZones);
                    Change::Partial
                }
                Err(error) => self.report(error),
            };
        }
        if matches!(input, Input::Tap(x, y) if defaults.contains(x, y)) {
            sizes = Sizes::default();
        } else if let Some((index, adjustment)) = (0..2).find_map(|index| {
            zone_size_controls(navigation.width, index)
                .1
                .delta(input, 5)
                .map(|delta| (index, delta))
        }) {
            let value = if index == 0 {
                &mut sizes.backward
            } else {
                &mut sizes.forward
            };
            *value = (i16::from(*value) + adjustment).clamp(0, 100) as u8;
        } else {
            return Change::None;
        }
        self.screen = Screen::PageTurnSizes(sizes);
        self.notice = None;
        Change::Partial
    }
}

fn zone_size_controls(width: i32, index: usize) -> (Rect, StepButtons) {
    let y = 312 + index as i32 * 136;
    (
        Rect::new(48, y, width - 396, 110),
        StepButtons::new(
            Rect::new(width - 320, y, 120, 110),
            Rect::new(width - 168, y, 120, 110),
        ),
    )
}

fn zone_size_actions(width: i32) -> (Rect, Rect) {
    let button_width = (width - 120) / 2;
    (
        Rect::new(48, 622, button_width, 100),
        Rect::new(width - 48 - button_width, 622, button_width, 100),
    )
}
