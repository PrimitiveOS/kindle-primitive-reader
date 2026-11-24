//! Compose enabled reading profiles when a document opens.
use super::{
    BookConfig, BookPreferences, FixedReading, Profile, Reader, Reading, ReadingPosition, Result,
};
use crate::session::ReadingViewport;

impl Reader {
    pub(in crate::app) fn toggle_opening_profile(&mut self, index: usize) -> Result<()> {
        let mut profiles = self.profiles.clone();
        let profile = profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?;
        profile.set_apply_on_open(!profile.applies_on_open());
        self.commit_profiles(profiles)?;
        self.notice = None;
        Ok(())
    }

    fn opening_profiles(&self) -> Vec<&Profile> {
        let mut profiles: Vec<_> = self
            .profiles
            .iter()
            .filter(|profile| profile.applies_on_open())
            .collect();
        profiles.sort_by_cached_key(|profile| profile.name().to_lowercase());
        profiles
    }

    pub(in crate::app) fn prepare_opening_text_profiles(
        &self,
        base: &BookConfig,
        preferences: &BookPreferences,
        position: Option<&ReadingPosition>,
        top_status_header: bool,
        target_viewport: ReadingViewport,
    ) -> Result<Option<(Reading, BookPreferences)>> {
        let mut adjusted = preferences.clone();
        for profile in self.opening_profiles() {
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
        if target_viewport != base.viewport {
            reading.prepare_viewport(target_viewport)?;
        }
        Ok(Some((reading, adjusted)))
    }

    pub(in crate::app) fn apply_opening_fixed_profiles(
        &self,
        reading: &mut FixedReading,
    ) -> Result<()> {
        let Some(provider) = reading.profile_provider() else {
            return Ok(());
        };
        let profiles: Vec<_> = self
            .opening_profiles()
            .into_iter()
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
