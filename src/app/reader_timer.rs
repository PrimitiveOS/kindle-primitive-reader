//! Fixed-interval page turns scheduled from reading inactivity.

use std::time::{Duration, Instant};

use super::Reader;
use crate::Result;

#[derive(Default)]
pub(super) struct AutoTurn {
    interval: Option<Duration>,
    deadline: Option<Instant>,
}

impl AutoTurn {
    pub(super) fn configure(&mut self, interval: Option<Duration>, now: Instant) {
        self.interval = interval.filter(|interval| !interval.is_zero());
        self.deadline = self.after(now);
    }

    pub(super) fn interval(&self) -> Option<Duration> {
        self.interval
    }

    pub(super) fn pause(&mut self) {
        self.deadline = None;
    }

    pub(super) fn resume(&mut self, now: Instant) {
        if self.deadline.is_none() {
            self.deadline = self.after(now);
        }
    }

    pub(super) fn activity(&mut self, now: Instant) {
        if self.deadline.is_some() {
            self.deadline = self.after(now);
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub(super) fn take_due(&mut self, now: Instant) -> bool {
        if !self.deadline.is_some_and(|deadline| now >= deadline) {
            return false;
        }
        self.activity(now);
        true
    }

    fn after(&self, now: Instant) -> Option<Instant> {
        self.interval.and_then(|interval| now.checked_add(interval))
    }
}

impl Reader {
    pub(super) fn set_fixed_auto_turn_overlap(
        &mut self,
        horizontal: bool,
        overlap: u8,
    ) -> Result<()> {
        if overlap > 90 {
            return Err("Zoom overlap must be from 0% to 90%".into());
        }
        let fixed = self
            .fixed
            .as_mut()
            .ok_or("Open a fixed-page document first")?;
        let preferences = fixed.reading.preferences();
        fixed.reading.set_auto_turn_pan(
            horizontal.then_some(overlap).or(preferences.zoom_overlap_h),
            (!horizontal)
                .then_some(overlap)
                .or(preferences.zoom_overlap_v),
            preferences.zoom_direction_vertical,
            preferences.zoom_bottom_to_top,
        )?;
        self.record_position();
        Ok(())
    }

    pub(super) fn change_auto_turn_pan(&mut self, change: super::AutoTurnPanChange) -> Result<()> {
        let fixed = self
            .fixed
            .as_mut()
            .ok_or("Open a fixed-page document first")?;
        let current = fixed.reading.preferences();
        let (horizontal, vertical, vertical_first, bottom_to_top) = match change {
            super::AutoTurnPanChange::ToggleVerticalFirst => (
                current.zoom_overlap_h,
                current.zoom_overlap_v,
                Some(!current.zoom_direction_vertical()),
                current.zoom_bottom_to_top,
            ),
            super::AutoTurnPanChange::ToggleBottomToTop => (
                current.zoom_overlap_h,
                current.zoom_overlap_v,
                current.zoom_direction_vertical,
                Some(!current.zoom_bottom_to_top()),
            ),
            super::AutoTurnPanChange::Reset => {
                let defaults = self
                    .document_defaults
                    .templates
                    .fixed
                    .unwrap_or_default()
                    .apply(current);
                (
                    Some(defaults.zoom_overlap_h()),
                    Some(defaults.zoom_overlap_v()),
                    Some(defaults.zoom_direction_vertical()),
                    Some(defaults.zoom_bottom_to_top()),
                )
            }
        };
        self.fixed.as_mut().unwrap().reading.set_auto_turn_pan(
            horizontal,
            vertical,
            vertical_first,
            bottom_to_top,
        )?;
        self.record_position();
        self.show(super::Screen::AutoTurnPan);
        Ok(())
    }

    pub(super) fn set_auto_turn_seconds(&mut self, seconds: u32) -> Result<()> {
        if !(1..=crate::reading_controls::MAX_AUTO_TURN_SECONDS).contains(&seconds) {
            return Err("Automatic page turns must be from 1 second to one day".into());
        }
        let controls = crate::reading_controls::ReadingControls {
            auto_turn_seconds: seconds,
            auto_turn_enabled: true,
            ..self.controls
        };
        controls.save(&self.controls_path)?;
        self.controls = controls;
        let now = Instant::now();
        self.auto_turn.configure(controls.auto_turn_interval(), now);
        self.sync_auto_turn(now);
        Ok(())
    }

    pub(super) fn disable_auto_turn(&mut self) -> Result<()> {
        let controls = crate::reading_controls::ReadingControls {
            auto_turn_enabled: false,
            ..self.controls
        };
        controls.save(&self.controls_path)?;
        self.controls = controls;
        let now = Instant::now();
        self.auto_turn.configure(None, now);
        self.sync_auto_turn(now);
        Ok(())
    }

    pub(super) fn adjust_auto_turn_distance(&mut self, delta_tenths: i16) -> Result<()> {
        let current = self.controls.auto_turn_distance_tenths();
        let mut next = current.saturating_add(delta_tenths).clamp(
            -crate::reading_controls::MAX_AUTO_TURN_DISTANCE_TENTHS,
            crate::reading_controls::MAX_AUTO_TURN_DISTANCE_TENTHS,
        );
        if next == 0 {
            next = if delta_tenths < 0 { -1 } else { 1 };
        }
        self.set_auto_turn_distance(next)
    }

    pub(super) fn reset_auto_turn_distance(&mut self) -> Result<()> {
        self.set_auto_turn_distance(10)
    }

    fn set_auto_turn_distance(&mut self, tenths: i16) -> Result<()> {
        if tenths == 0
            || tenths.unsigned_abs() > crate::reading_controls::MAX_AUTO_TURN_DISTANCE_TENTHS as u16
        {
            return Err("Automatic scroll distance must be between −20.0 and +20.0 pages".into());
        }
        let controls = crate::reading_controls::ReadingControls {
            auto_turn_distance_tenths: tenths,
            ..self.controls
        };
        controls.save(&self.controls_path)?;
        self.controls = controls;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_secs(10);

    #[test]
    fn emits_one_turn_after_a_late_tick() {
        let now = Instant::now();
        let mut timer = AutoTurn::default();
        timer.configure(Some(INTERVAL), now);
        assert_eq!(timer.deadline(), Some(now + INTERVAL));
        assert!(!timer.take_due(now));
        let delayed = now + INTERVAL * 4;
        assert!(timer.take_due(delayed));
        assert!(!timer.take_due(delayed));
        assert_eq!(timer.deadline(), Some(delayed + INTERVAL));
    }

    #[test]
    fn activity_restarts_an_active_interval_and_leaves_a_paused_timer_paused() {
        let now = Instant::now();
        let mut timer = AutoTurn::default();
        timer.configure(Some(INTERVAL), now);
        let input = now + Duration::from_secs(3);
        timer.activity(input);
        timer.resume(now + Duration::from_secs(4));
        assert_eq!(timer.deadline(), Some(input + INTERVAL));
        assert!(!timer.take_due(now + INTERVAL));
        timer.pause();
        let resumed = now + INTERVAL * 2;
        timer.activity(resumed);
        assert_eq!(timer.interval(), Some(INTERVAL));
        assert_eq!(timer.deadline(), None);
        assert!(!timer.take_due(resumed));
        timer.resume(resumed);
        assert!(!timer.take_due(resumed));
        assert!(timer.take_due(resumed + INTERVAL));
    }

    #[test]
    fn disabling_the_timer_cancels_the_deadline_and_prevents_restart() {
        let now = Instant::now();
        let mut timer = AutoTurn::default();
        timer.configure(Some(INTERVAL), now);
        timer.configure(None, now);
        assert_eq!(timer.interval(), None);
        timer.resume(now + INTERVAL);
        assert_eq!(timer.deadline(), None);
        assert!(!timer.take_due(now + INTERVAL * 2));
        timer.configure(Some(Duration::ZERO), now);
        assert_eq!(timer.interval(), None);
    }
}
