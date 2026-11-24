//! Compose enabled reading profiles when a document opens.
use super::{
    BookConfig, BookPreferences, FixedReading, Profile, Reader, Reading, ReadingPosition, Result,
};
use crate::reading_profiles::opening::Book;
use crate::session::ReadingViewport;
use std::path::Path;

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

    pub(in crate::app) fn prepare_opening_text_profiles(
        &self,
        base: &BookConfig,
        preferences: &BookPreferences,
        position: Option<&ReadingPosition>,
        top_status_header: bool,
        id: u64,
    ) -> Result<Option<(Reading, BookPreferences)>> {
        let orientation = self.opening_orientation(id, false);
        let metadata = self.opening_text_metadata(base, position.is_none());
        let book = Book {
            id,
            path: &base.book,
            library: &self.library,
            orientation,
            is_new: position.is_none(),
            metadata: metadata.as_ref(),
        };
        let mut adjusted = preferences.clone();
        for profile in self.opening_profiles(&book) {
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
    ) -> Result<()> {
        let Some(provider) = reading.profile_provider() else {
            return Ok(());
        };
        let metadata = self.opening_fixed_metadata(reading, path, is_new);
        let book = Book {
            id,
            path,
            library: &self.library,
            orientation: self.opening_orientation(id, true),
            is_new,
            metadata: metadata.as_ref(),
        };
        let profiles: Vec<_> = self
            .opening_profiles(&book)
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
