use super::{Action, Item, Reader};
use crate::Result;
use primitive_app::kindle_device::LEVEL_MAX;

#[derive(Clone, Copy)]
pub(super) enum DisplayLevel {
    FrontLight,
    Warmth,
}

impl DisplayLevel {
    fn title(self) -> &'static str {
        match self {
            Self::FrontLight => "Front light",
            Self::Warmth => "Warmth",
        }
    }
}

#[derive(Default)]
pub(super) struct QuickDisplay {
    light: Option<u32>,
    warmth: Option<u32>,
}

impl QuickDisplay {
    pub fn read() -> Self {
        #[cfg(all(target_os = "linux", target_arch = "arm"))]
        {
            use primitive_app::kindle_device::{light_level, warmth_level};
            Self {
                light: light_level(),
                warmth: warmth_level(),
            }
        }
        #[cfg(not(all(target_os = "linux", target_arch = "arm")))]
        {
            Self {
                light: Some(12),
                warmth: Some(0),
            }
        }
    }

    pub(super) fn level(&self, control: DisplayLevel) -> Option<u32> {
        match control {
            DisplayLevel::FrontLight => self.light,
            DisplayLevel::Warmth => self.warmth,
        }
    }

    pub fn set(&mut self, control: DisplayLevel, requested: u32) -> Result<()> {
        let current = self
            .level(control)
            .ok_or_else(|| format!("{} is unavailable", control.title()))?;
        let level = requested.min(LEVEL_MAX);
        if level == current {
            return Ok(());
        }
        #[cfg(all(target_os = "linux", target_arch = "arm"))]
        {
            use primitive_app::kindle_device::{try_set_light, try_set_warmth};
            let updated = match control {
                DisplayLevel::FrontLight => try_set_light(level),
                DisplayLevel::Warmth => try_set_warmth(level),
            };
            if !updated {
                return Err(format!("Could not adjust {}", control.title()).into());
            }
        }
        match control {
            DisplayLevel::FrontLight => self.light = Some(level),
            DisplayLevel::Warmth => self.warmth = Some(level),
        }
        Ok(())
    }

    pub fn swipe(&mut self, control: DisplayLevel, dx: f32) -> Result<()> {
        let current = self
            .level(control)
            .ok_or_else(|| format!("{} is unavailable", control.title()))?;
        let movement = (dx / 48.0).round() as i32;
        let target = (current as i32 + movement).clamp(0, LEVEL_MAX as i32) as u32;
        self.set(control, target)
    }
}

impl Reader {
    pub(super) fn quick_display_items(&self) -> Vec<Item> {
        let current = self.controls;
        let mut items = Vec::new();
        // Under Primitive Launcher the theme is the launcher's: no light or dark switch here.
        if !self.under_launcher() {
            items.push(
                Item::new(
                    "Interface palette",
                    Action::SetReadingControls(crate::reading_controls::ReadingControls {
                        palette: current.palette.next(),
                        ..current
                    }),
                )
                .detail(current.palette.label()),
            );
        }
        items.extend([
            Item::new(
                "Screen orientation",
                Action::SetReadingControls(crate::reading_controls::ReadingControls {
                    orientation: current.orientation.next(),
                    ..current
                }),
            )
            .detail(current.orientation.label()),
            Item::new("Refresh screen now", Action::RefreshDisplay),
            Item::new(
                "Reading status bar",
                Action::Show(super::Screen::FooterSettings),
            )
            .detail(current.status_bar.label()),
        ]);
        for control in [DisplayLevel::FrontLight, DisplayLevel::Warmth] {
            if let Some(level) = self.quick_display.level(control) {
                let detail = format!("{level} / {LEVEL_MAX}");
                items.push(
                    Item::new(
                        format!("{} −", control.title()),
                        Action::SetDisplayLevel(control, level.saturating_sub(1)),
                    )
                    .detail(&detail),
                );
                items.push(
                    Item::new(
                        format!("{} +", control.title()),
                        Action::SetDisplayLevel(control, (level + 1).min(LEVEL_MAX)),
                    )
                    .detail(&detail),
                );
            }
        }
        items
    }
}
