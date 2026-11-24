//! Named profiles share the normal document-specific reading settings.
use primitive_ui::{Sheet, SheetAction};

use super::*;
use crate::reading_profiles::PdfProfile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Name {
    Create,
    Rename(usize),
    Duplicate(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Confirm {
    Replace,
    Delete,
}

impl Name {
    pub(super) fn back(self) -> Screen {
        match self {
            Self::Create => Screen::ReadingProfiles,
            Self::Rename(index) | Self::Duplicate(index) => Screen::ReadingProfile(index),
        }
    }
}

impl Reader {
    pub(super) fn pdf_profiles_available(&self) -> bool {
        self.fixed
            .as_ref()
            .is_some_and(|book| book.entry.format == DocumentFormat::Pdf)
    }

    fn profile_supported(&self, profile: &Profile) -> bool {
        match profile {
            Profile::Text { .. } => self.active.is_some(),
            Profile::Pdf { .. } => self.pdf_profiles_available(),
        }
    }

    pub(super) fn reading_profile_items(&self) -> Vec<Item> {
        if self.screen == Screen::ReadingProfiles {
            let mut items = Vec::new();
            if self.pdf_profiles_available() || self.active.is_some() {
                items.push(
                    Item::new(
                        "Save current settings",
                        Action::Show(Screen::ProfileName(Name::Create)),
                    )
                    .detail(if self.pdf_profiles_available() {
                        "PDF page settings"
                    } else {
                        "Reflowable book settings"
                    }),
                );
            }
            let mut ordered: Vec<_> = self.profiles.iter().enumerate().collect();
            ordered.sort_by_cached_key(|(_, profile)| profile.name().to_lowercase());
            items.extend(ordered.into_iter().map(|(index, profile)| {
                Item::new(profile.name(), Action::Show(Screen::ReadingProfile(index)))
                    .detail(profile_format(profile))
            }));
            return items;
        }
        let (Screen::ReadingProfile(index) | Screen::ConfirmProfile(index, _)) = self.screen else {
            return Vec::new();
        };
        let Some(profile) = self.profiles.get(index) else {
            return Vec::new();
        };
        let mut items =
            vec![Item::new(profile.name(), Action::Heading).detail(profile_format(profile))];
        if self.profile_supported(profile) {
            items.extend([
                Item::new("Apply to this book", Action::ApplyProfile(index)),
                Item::new(
                    "Replace with current settings",
                    Action::Show(Screen::ConfirmProfile(index, Confirm::Replace)),
                ),
            ]);
        } else {
            items.push(
                Item::new("Apply to this book", Action::Heading).detail(match profile {
                    Profile::Text { .. } => "Open a reflowable book to use this profile",
                    Profile::Pdf { .. } => "Open a PDF to use this profile",
                }),
            );
        }
        items.extend([
            Item::new(
                "Rename",
                Action::Show(Screen::ProfileName(Name::Rename(index))),
            ),
            Item::new(
                "Duplicate",
                Action::Show(Screen::ProfileName(Name::Duplicate(index))),
            ),
            Item::new(
                "Delete",
                Action::Show(Screen::ConfirmProfile(index, Confirm::Delete)),
            ),
        ]);
        items
    }

    fn capture_profile(&self, name: &str) -> Result<Profile> {
        if let Some(fixed) = self
            .fixed
            .as_ref()
            .filter(|_| self.pdf_profiles_available())
        {
            return Ok(Profile::Pdf {
                name: name.into(),
                fixed: PdfProfile::from_view(fixed.reading.preferences()),
            });
        }
        Ok(Profile::Text {
            name: name.into(),
            preferences: self.current_text_preferences()?,
        })
    }

    fn commit_profiles(&mut self, profiles: Vec<Profile>) -> Result<()> {
        reading_profiles::save(&self.profiles_path, &self.profiles, &profiles)?;
        self.profiles = profiles;
        Ok(())
    }

    pub(super) fn save_reading_profile(&mut self, name: &str) -> Result<()> {
        let mut profiles = self.profiles.clone();
        profiles.push(self.capture_profile(name)?);
        self.commit_profiles(profiles)?;
        self.show(Screen::ReadingProfiles);
        self.notice = Some("Reading profile saved".into());
        Ok(())
    }

    pub(super) fn edit_reading_profile_name(&mut self, mode: Name, name: &str) -> Result<()> {
        let index = match mode {
            Name::Create => return self.save_reading_profile(name),
            Name::Rename(index) | Name::Duplicate(index) => index,
        };
        let mut profiles = self.profiles.clone();
        let profile = profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?;
        match mode {
            Name::Rename(_) => profile.set_name(name),
            Name::Duplicate(_) => {
                let mut copy = profile.clone();
                copy.set_name(name);
                profiles.push(copy);
            }
            Name::Create => unreachable!(),
        }
        self.commit_profiles(profiles)?;
        self.show(match mode {
            Name::Rename(_) => Screen::ReadingProfile(index),
            _ => Screen::ReadingProfiles,
        });
        Ok(())
    }

    pub(super) fn update_reading_profile(&mut self, index: usize) -> Result<()> {
        let previous = self
            .profiles
            .get(index)
            .ok_or("That profile is unavailable")?;
        if !self.profile_supported(previous) {
            return Err("Open a book that matches this profile's format".into());
        }
        let profile = self.capture_profile(previous.name())?;
        let mut profiles = self.profiles.clone();
        profiles[index] = profile;
        self.commit_profiles(profiles)?;
        self.show(Screen::ReadingProfile(index));
        self.notice = Some("Reading profile updated".into());
        Ok(())
    }

    pub(super) fn remove_reading_profile(&mut self, index: usize) -> Result<()> {
        let mut profiles = self.profiles.clone();
        if index >= profiles.len() {
            return Err("That profile is unavailable".into());
        }
        profiles.remove(index);
        self.commit_profiles(profiles)?;
        self.show(Screen::ReadingProfiles);
        Ok(())
    }

    pub(super) fn apply_reading_profile(&mut self, index: usize) -> Result<()> {
        let profile = self
            .profiles
            .get(index)
            .ok_or("That profile is unavailable")?;
        if !self.profile_supported(profile) {
            return Err("Open a book that matches this profile's format".into());
        }
        match profile {
            Profile::Text { preferences, .. } => self.apply_text_profile(preferences.clone()),
            Profile::Pdf { fixed, .. } => self.apply_pdf_profile(*fixed),
        }
    }

    fn apply_pdf_profile(&mut self, profile: PdfProfile) -> Result<()> {
        let fixed = self.fixed.as_mut().ok_or("Open a PDF first")?;
        let library = &mut self.library;
        let entry = &fixed.entry;
        let id = fixed.id;
        let original_page = fixed.reading.page();
        let prior = library
            .state(id)
            .cloned()
            .ok_or("Reading state unavailable")?;
        let result = fixed
            .reading
            .restore_pdf_profile_with(profile, |page, effective| {
                if page != original_page {
                    return Err(io::Error::other(
                        "This profile would move to a different page",
                    ));
                }
                with_storage(|| {
                    if library.identify(entry)? != id {
                        return Err(io::Error::other(
                            "The PDF changed. Reopen it before applying the profile.",
                        ));
                    }
                    let state = library.state_mut(id).unwrap();
                    state.fixed_preferences = effective;
                    state.position = Some(ReadingPosition {
                        chapter: 0,
                        page,
                        anchor: None,
                    });
                    library.persist()
                })
            });
        if let Err(error) = result {
            *library.state_mut(id).unwrap() = prior;
            return Err(error);
        }
        fixed.reading.pause();
        self.show(Screen::Fixed);
        self.record_position();
        Ok(())
    }

    pub(super) fn draw_profile_confirmation(&self, canvas: &mut Canvas) {
        if let Screen::ConfirmProfile(index, confirm) = self.screen
            && let Some(profile) = self.profiles.get(index)
        {
            profile_confirmation_sheet(profile.name(), confirm).draw(canvas, None);
        }
    }

    pub(super) fn profile_confirmation_input(&mut self, input: Input) -> Option<Change> {
        let Screen::ConfirmProfile(index, confirm) = self.screen else {
            return None;
        };
        if let Some(profile) = self.profiles.get(index) {
            let choice = match input {
                Input::PageBack => Some(SheetAction::Cancel),
                Input::Tap(x, y) => profile_confirmation_sheet(profile.name(), confirm).hit(x, y),
                _ => None,
            };
            match choice {
                Some(SheetAction::Cancel) => self.show(Screen::ReadingProfile(index)),
                Some(SheetAction::Confirm) => {
                    return Some(
                        match self.activate(match confirm {
                            Confirm::Replace => Action::UpdateReadingProfile(index),
                            Confirm::Delete => Action::RemoveProfile(index),
                        }) {
                            Ok(()) => Change::Partial,
                            Err(error) => {
                                self.show(Screen::ReadingProfile(index));
                                self.report(error)
                            }
                        },
                    );
                }
                None => return Some(Change::None),
            }
        } else {
            self.show(Screen::ReadingProfiles);
        }
        Some(Change::Partial)
    }
}

fn profile_confirmation_sheet(name: &str, confirm: Confirm) -> Sheet<'_> {
    let view = Rect::new(0, 0, WIDTH as i32, HEIGHT as i32);
    match confirm {
        Confirm::Replace => Sheet::centered(view, "Replace this profile?", name, "Replace"),
        Confirm::Delete => Sheet::centered(view, "Delete this profile?", name, "Delete profile"),
    }
}

fn profile_format(profile: &Profile) -> &'static str {
    match profile {
        Profile::Text { .. } => "Reflowable books",
        Profile::Pdf { .. } => "PDF documents",
    }
}

#[cfg(test)]
mod tests;
