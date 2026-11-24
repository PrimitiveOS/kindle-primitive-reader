//! Compose enabled reading profiles when a document opens.
use super::{
    BookConfig, BookPreferences, FixedReading, Profile, Reader, Reading, ReadingOrientation,
    ReadingPosition, Result,
};
use crate::reading_profiles::FixedProvider;
use crate::reading_profiles::opening::{Book, Metadata};
use crate::session::ReadingViewport;
use primitive_app::viewport::{LogicalTurn, ViewportRequest};
use std::path::Path;

const TURNS: [(LogicalTurn, ReadingOrientation); 4] = [
    (LogicalTurn::Upright, ReadingOrientation::Upright),
    (LogicalTurn::UpsideDown, ReadingOrientation::UpsideDown),
    (
        LogicalTurn::Counterclockwise,
        ReadingOrientation::LandscapeLeft,
    ),
    (LogicalTurn::Clockwise, ReadingOrientation::LandscapeRight),
];

pub(in crate::app) struct OpeningPlan {
    pub(in crate::app) epoch: u64,
    pub(in crate::app) fallback: Option<ViewportRequest>,
    matching: Vec<(u8, Profile)>,
    pub(in crate::app) text: Option<(BookPreferences, bool)>,
}

impl OpeningPlan {
    pub(in crate::app) fn profiles(&self, turn: LogicalTurn) -> impl Iterator<Item = &Profile> {
        let index = TURNS
            .iter()
            .position(|&(mode, _)| mode == turn)
            .expect("known turn");
        self.matching
            .iter()
            .filter_map(move |(mask, profile)| (mask & (1 << index) != 0).then_some(profile))
    }
}

impl Reader {
    fn opening_profiles(&self, book: &Book<'_>) -> Vec<&Profile> {
        let mut profiles: Vec<_> = self
            .profiles
            .iter()
            .filter(|profile| profile.matches_opening(book))
            .collect();
        profiles.sort_by_cached_key(|profile| profile.name().to_lowercase());
        profiles
    }

    /// Snapshot all alternatives from the same opening event. A physical rule
    /// and a path match for the same profile still execute only once.
    pub(in crate::app) fn opening_profile_plan(
        &self,
        id: u64,
        path: &Path,
        is_new: bool,
        fixed: Option<FixedProvider>,
        metadata: Option<&Metadata>,
        local_minute: Option<u16>,
    ) -> Option<OpeningPlan> {
        let fits = |p: &Profile| match p {
            Profile::Text { .. } => fixed.is_none(),
            Profile::Fixed { fixed: profile, .. } => Some(profile.provider) == fixed,
        };
        if !self.negotiated_opening_profiles {
            return None;
        }
        let preferred = self.opening_orientation(id, fixed.is_some());
        let context = |orientation| Book {
            id,
            path,
            library: &self.library,
            orientation,
            is_new,
            metadata,
            local_minute,
            download_folder: self.browser.directory(),
        };
        let mut matching: Vec<_> = self
            .profiles
            .iter()
            .filter(|p| fits(p))
            .filter_map(|profile| {
                let mask = if preferred == ReadingOrientation::FollowLauncher
                    && profile.matches_opening(&context(preferred))
                {
                    0b1111
                } else {
                    TURNS
                        .into_iter()
                        .enumerate()
                        .fold(0, |mask, (i, (_, orientation))| {
                            mask | (u8::from(profile.matches_opening(&context(orientation))) << i)
                        })
                };
                (mask != 0).then(|| (mask, profile.clone()))
            })
            .collect();
        if !matching.iter().any(|(_, profile)| {
            profile.needs_opening_queue()
                || profile.opening_conditions().is_some_and(|rule| {
                    rule.orientations
                        .iter()
                        .any(|mode| *mode != ReadingOrientation::FollowLauncher)
                })
        }) {
            return None;
        }
        matching.sort_by_cached_key(|(_, profile)| profile.name().to_lowercase());
        Some(OpeningPlan {
            epoch: 0,
            fallback: None,
            matching,
            text: None,
        })
    }

    pub(in crate::app) fn legacy_opening_queue(
        &self,
        id: u64,
        path: &Path,
        is_new: bool,
        fixed: Option<FixedProvider>,
        metadata: Option<&Metadata>,
        local_minute: Option<u16>,
    ) -> Vec<Profile> {
        let book = Book {
            id,
            path,
            library: &self.library,
            orientation: self.opening_orientation(id, fixed.is_some()),
            is_new,
            metadata,
            local_minute,
            download_folder: self.browser.directory(),
        };
        self.opening_profiles(&book)
            .into_iter()
            .filter(|profile| profile.needs_opening_queue())
            .filter(|profile| match profile {
                Profile::Text { .. } => fixed.is_none(),
                Profile::Fixed {
                    fixed: settings, ..
                } => Some(settings.provider) == fixed,
            })
            .cloned()
            .collect()
    }

    pub(in crate::app) fn install_opening_profile_plan(&mut self, mut plan: Option<OpeningPlan>) {
        self.opening_profile_epoch = self.opening_profile_epoch.wrapping_add(1);
        if self.pending_opening_profiles.is_some() || plan.is_some() {
            self.staged_viewport = None;
        }
        if let Some(plan) = &mut plan {
            plan.epoch = self.opening_profile_epoch;
        }
        self.pending_opening_profiles = plan;
    }

    pub(in crate::app) fn settle_opening_location(&mut self) {
        self.reset_footer_initial_position();
        self.remember_opening_location();
        self.record_position();
        self.sync_auto_turn(std::time::Instant::now());
    }

    pub(in crate::app) fn settle_pending_opening(&mut self) {
        if self.pending_opening_profiles.is_some() {
            self.install_opening_profile_plan(None);
            self.settle_opening_location();
        }
    }

    pub(in crate::app) fn opening_display_viewport(&self) -> ReadingViewport {
        if self.accepted_viewport.is_some_and(|accepted| {
            accepted.logical_size == ReadingViewport::LANDSCAPE.dimensions()
        }) {
            ReadingViewport::LANDSCAPE
        } else {
            ReadingViewport::PORTRAIT
        }
    }

    pub(in crate::app) fn prepare_opening_profile_text_for_turn(
        &self,
        viewport: ReadingViewport,
        turn: LogicalTurn,
    ) -> Result<Option<(Reading, BookPreferences)>> {
        let Some(plan) = &self.pending_opening_profiles else {
            return Ok(None);
        };
        let Some((preferences, top_header)) = &plan.text else {
            return Ok(None);
        };
        let mut adjusted = preferences.clone();
        for profile in plan
            .profiles(turn)
            .filter(|profile| !profile.needs_opening_queue())
        {
            if let Some(projected) = profile.apply_text(&adjusted) {
                adjusted = projected;
            }
        }
        if adjusted == *preferences {
            return Ok(None);
        }
        let current = &self
            .active
            .as_ref()
            .ok_or("The book closed before opening")?
            .reading;
        let mut config = current.config.clone();
        config.viewport = viewport;
        config.screen_pages = Default::default();
        config.settings = adjusted.apply_to(self.device_reading_settings_for(viewport));
        config.settings.status_header_height =
            crate::reading_controls::TopHeader::height(*top_header, adjusted.scroll);
        config.scroll = adjusted.scroll;
        config.facing_pages = adjusted.facing_pages;
        if adjusted.font_file != preferences.font_file {
            config.fonts = self.fonts_for(adjusted.font_file.as_deref())?;
        }
        let anchor = current.anchor();
        let reading = Reading::restore_with_scroll_top(
            config,
            current.chapter,
            current.page,
            anchor.as_ref(),
            current.scroll_history_top(),
        )?;
        if reading.anchor() != anchor || (anchor.is_none() && reading.page != current.page) {
            return Err("This profile could not keep the saved reading position".into());
        }
        Ok(Some((reading, adjusted)))
    }

    pub(in crate::app) fn prepare_opening_text_profiles(
        &self,
        base: &BookConfig,
        preferences: &BookPreferences,
        position: Option<&ReadingPosition>,
        top_status_header: bool,
        id: u64,
        metadata: Option<&Metadata>,
        local_minute: Option<u16>,
    ) -> Result<Option<(Reading, BookPreferences)>> {
        let orientation = self.opening_orientation(id, false);
        let book = Book {
            id,
            path: &base.book,
            library: &self.library,
            orientation,
            is_new: position.is_none(),
            metadata,
            local_minute,
            download_folder: self.browser.directory(),
        };
        let mut adjusted = preferences.clone();
        for profile in self
            .opening_profiles(&book)
            .into_iter()
            .filter(|profile| !profile.needs_opening_queue())
        {
            if let Some(projected) = profile.apply_text(&adjusted) {
                adjusted = projected;
            }
        }
        if &adjusted == preferences {
            return Ok(None);
        }
        let mut config = base.clone();
        config.settings = adjusted.apply_to(self.device_reading_settings_for(base.viewport));
        config.settings.status_header_height =
            crate::reading_controls::TopHeader::height(top_status_header, adjusted.scroll);
        if adjusted.font_file != preferences.font_file {
            config.fonts = self.fonts_for(adjusted.font_file.as_deref())?;
        }
        config.scroll = adjusted.scroll;
        config.facing_pages = adjusted.facing_pages;
        let reading = if let Some(saved) = position {
            let legacy_anchor;
            let anchor = if let Some(anchor) = saved.anchor.as_ref() {
                anchor
            } else {
                let baseline = Reading::restore(base.clone(), saved.chapter, saved.page, None)?;
                legacy_anchor = baseline
                    .anchor()
                    .ok_or("The saved page could not be resolved to a source position")?;
                &legacy_anchor
            };
            Reading::restore(config, saved.chapter, saved.page, Some(anchor))?
        } else {
            Reading::open(config)?
        };
        if orientation.is_landscape() && base.viewport != ReadingViewport::LANDSCAPE {
            reading.prepare_viewport(ReadingViewport::LANDSCAPE)?;
        }
        Ok(Some((reading, adjusted)))
    }

    pub(in crate::app) fn apply_opening_fixed_profiles(
        &self,
        reading: &mut FixedReading,
        id: u64,
        path: &Path,
        is_new: bool,
        metadata: Option<&Metadata>,
        local_minute: Option<u16>,
    ) -> Result<()> {
        let Some(provider) = reading.profile_provider() else {
            return Ok(());
        };
        let book = Book {
            id,
            path,
            library: &self.library,
            orientation: self.opening_orientation(id, true),
            is_new,
            metadata,
            local_minute,
            download_folder: self.browser.directory(),
        };
        let profiles: Vec<_> = self
            .opening_profiles(&book)
            .into_iter()
            .filter(|profile| !profile.needs_opening_queue())
            .filter_map(|profile| match profile {
                Profile::Fixed { fixed, .. } if fixed.provider == provider => {
                    Some((*fixed, profile.selection()))
                }
                _ => None,
            })
            .collect();
        if profiles.is_empty() {
            return Ok(());
        }
        let original_page = reading.page();
        reading.restore_fixed_profiles_with(
            profiles
                .iter()
                .map(|(fixed, fields)| (*fixed, fields.as_slice())),
            |page, _| {
                if page != original_page {
                    return Err(std::io::Error::other(
                        "This profile would move to a different page",
                    ));
                }
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests;
