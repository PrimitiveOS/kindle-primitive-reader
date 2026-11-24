//! Named profiles share the normal document-specific reading settings.
use primitive_ui::{Sheet, SheetAction};

use super::{navigation_layout::NavigationLayout, *};
use crate::reading_profiles::{Field, FixedProfile};

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
    pub(super) fn fixed_profiles_available(&self) -> bool {
        self.fixed
            .as_ref()
            .and_then(|book| book.reading.profile_provider())
            .is_some()
    }

    fn profile_supported(&self, profile: &Profile) -> bool {
        match profile {
            Profile::Text { .. } => self.active.is_some(),
            Profile::Fixed { fixed, .. } => {
                self.fixed
                    .as_ref()
                    .and_then(|book| book.reading.profile_provider())
                    == Some(fixed.provider)
            }
        }
    }

    pub(super) fn reading_profile_items(&self) -> Vec<Item> {
        if self.screen == Screen::ReadingProfiles {
            let mut items = Vec::new();
            if self.fixed_profiles_available() || self.active.is_some() {
                items.push(
                    Item::new(
                        "Save current settings",
                        Action::Show(Screen::ProfileName(Name::Create)),
                    )
                    .detail(
                        self.fixed
                            .as_ref()
                            .and_then(|book| book.reading.profile_provider())
                            .map_or("Reflowable books", |provider| provider.label()),
                    ),
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
        if let Screen::ReadingProfileValues(index) | Screen::ReadingProfileMargins(index) =
            self.screen
        {
            return self.profile_value_items(index);
        }
        if let Screen::ReadingProfileFields(index) = self.screen {
            return self.profile_field_items(index);
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
                    Profile::Fixed { .. } => "Open a matching document to use this profile",
                }),
            );
        }
        items.extend([
            Item::new("Included settings", Action::EditReadingProfileFields(index)).detail(
                format!(
                    "{} of {}",
                    profile.selection().len(),
                    profile.available().len()
                ),
            ),
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

    fn profile_field_items(&self, index: usize) -> Vec<Item> {
        let Some(profile) = self.profiles.get(index) else {
            return Vec::new();
        };
        let mut items = vec![
            Item::new("Select all", Action::SetReadingProfileFields(true)),
            Item::new("Clear all", Action::SetReadingProfileFields(false)),
        ];
        items.extend(
            profile
                .available()
                .into_iter()
                .map(|field| Item::new(field.label(), Action::ToggleReadingProfileField(field))),
        );
        items
    }

    pub(super) fn edit_profile_fields(&mut self, index: usize) -> Result<()> {
        let selection = self
            .profiles
            .get(index)
            .ok_or("That profile is unavailable")?
            .selection();
        self.show(Screen::ReadingProfileFields(index));
        self.profile_fields = selection;
        Ok(())
    }

    fn fields_profile(&self) -> Result<&Profile> {
        let Screen::ReadingProfileFields(index) = self.screen else {
            return Err("Open included settings first".into());
        };
        self.profiles
            .get(index)
            .ok_or_else(|| "That profile is unavailable".into())
    }

    pub(super) fn toggle_profile_field(&mut self, field: Field) -> Result<()> {
        if !self.fields_profile()?.available().contains(&field) {
            return Err("That setting is unavailable".into());
        }
        if let Some(index) = self.profile_fields.iter().position(|&entry| entry == field) {
            self.profile_fields.remove(index);
        } else {
            self.profile_fields.push(field);
        }
        self.notice = None;
        Ok(())
    }

    pub(super) fn set_profile_fields(&mut self, all: bool) -> Result<()> {
        let profile = self.fields_profile()?;
        self.profile_fields = if all { profile.available() } else { Vec::new() };
        self.notice = None;
        Ok(())
    }

    pub(super) fn draw_profile_field_controls(&self, canvas: &mut Canvas) {
        if matches!(self.screen, Screen::ReadingProfileFields(_)) {
            let (_, cancel) = field_buttons(self.navigation_layout());
            canvas.button(cancel, "Cancel", false, false);
        }
    }

    pub(super) fn profile_field_controls_input(&mut self, input: Input) -> Option<Change> {
        let Screen::ReadingProfileFields(index) = self.screen else {
            return None;
        };
        let Input::Tap(x, y) = input else { return None };
        let (save, cancel) = field_buttons(self.navigation_layout());
        if cancel.contains(x, y) {
            self.show(Screen::ReadingProfile(index));
            return Some(Change::Partial);
        }
        if save.contains(x, y) {
            return Some(match self.save_profile_fields(index) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            });
        }
        None
    }

    fn save_profile_fields(&mut self, index: usize) -> Result<()> {
        let mut profiles = self.profiles.clone();
        profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?
            .set_fields(self.profile_fields.clone())?;
        self.commit_profiles(profiles)?;
        self.show(Screen::ReadingProfile(index));
        Ok(())
    }

    fn capture_profile(&self, name: &str) -> Result<Profile> {
        if let Some(book) = &self.fixed
            && let Some(provider) = book.reading.profile_provider()
        {
            return Ok(Profile::Fixed {
                name: name.into(),
                fixed: FixedProfile::from_view(provider, book.reading.preferences()),
                fields: None,
            });
        }
        Ok(Profile::Text {
            name: name.into(),
            preferences: self.current_text_preferences()?,
            fields: None,
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
        let snapshot = self.capture_profile(previous.name())?;
        let mut profiles = self.profiles.clone();
        profiles[index].update_from(snapshot)?;
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
            Profile::Text { .. } => {
                let active = self.active.as_ref().ok_or("Open a book first")?;
                let current = &self
                    .library
                    .state(active.id)
                    .ok_or("Reading state unavailable")?
                    .preferences;
                let preferences = profile
                    .apply_text(current)
                    .ok_or("That profile is unavailable")?;
                self.apply_text_profile(preferences)
            }
            Profile::Fixed { fixed, .. } => self.apply_fixed_profile(*fixed, &profile.selection()),
        }
    }

    fn apply_fixed_profile(&mut self, profile: FixedProfile, fields: &[Field]) -> Result<()> {
        let fixed = self
            .fixed
            .as_mut()
            .ok_or("Open a fixed-page document first")?;
        let library = &mut self.library;
        let entry = &fixed.entry;
        let id = fixed.id;
        let original_page = fixed.reading.page();
        let prior = library
            .state(id)
            .cloned()
            .ok_or("Reading state unavailable")?;
        let result =
            fixed
                .reading
                .restore_fixed_profile_with(profile, fields, |page, effective| {
                    if page != original_page {
                        return Err(io::Error::other(
                            "This profile would move to a different page",
                        ));
                    }
                    with_storage(|| {
                        if library.identify(entry)? != id {
                            return Err(io::Error::other(
                                "The document changed. Reopen it before applying the profile.",
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
            profile_confirmation_sheet(self.navigation_layout(), profile.name(), confirm)
                .draw(canvas, None);
        }
    }

    pub(super) fn profile_confirmation_input(&mut self, input: Input) -> Option<Change> {
        let Screen::ConfirmProfile(index, confirm) = self.screen else {
            return None;
        };
        if let Some(profile) = self.profiles.get(index) {
            let choice = match input {
                Input::PageBack => Some(SheetAction::Cancel),
                Input::Tap(x, y) => {
                    profile_confirmation_sheet(self.navigation_layout(), profile.name(), confirm)
                        .hit(x, y)
                }
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

fn field_buttons(layout: NavigationLayout) -> (Rect, Rect) {
    let save = layout.back();
    (
        save,
        Rect {
            x: save.x - save.w - 12,
            ..save
        },
    )
}

fn profile_confirmation_sheet(layout: NavigationLayout, name: &str, confirm: Confirm) -> Sheet<'_> {
    let short_edge = layout.width.min(layout.height);
    let view = Rect::new(
        (layout.width - short_edge) / 2,
        0,
        short_edge,
        layout.height,
    );
    match confirm {
        Confirm::Replace => Sheet::centered(view, "Replace this profile?", name, "Replace"),
        Confirm::Delete => Sheet::centered(view, "Delete this profile?", name, "Delete profile"),
    }
}

fn profile_format(profile: &Profile) -> &'static str {
    match profile {
        Profile::Text { .. } => "Reflowable books",
        Profile::Fixed { fixed, .. } => fixed.provider.label(),
    }
}

mod values;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod viewport_tests;
