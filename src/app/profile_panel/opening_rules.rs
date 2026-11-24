//! Editing the conditions for applying a profile when a book opens.
use super::{Action, Editor, Item, Name, Profile, Reader, ReadingOrientation, Result, Screen};
use crate::library::fold_search;
use crate::reading_profiles::{MAX_NAME, opening::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Page {
    Overview,
    Orientation,
    Collections,
}

impl Page {
    pub(in crate::app) fn title(self) -> &'static str {
        match self {
            Self::Overview => "Apply on book opening",
            Self::Orientation => "Opening orientations",
            Self::Collections => "Opening collections",
        }
    }

    pub(in crate::app) fn back(self, index: usize) -> Screen {
        match self {
            Self::Overview => Screen::ReadingProfile(index),
            _ => Screen::ReadingProfileOpening(index, Self::Overview),
        }
    }
}

#[derive(Clone)]
pub(in crate::app) enum Command {
    Always,
    Orientation(ReadingOrientation),
    Collection(String),
    NewOnly,
    Clear,
}

pub(super) fn summary(profile: &Profile) -> &'static str {
    if profile.applies_on_open() {
        "Always"
    } else if profile
        .opening_conditions()
        .is_some_and(Conditions::has_matchers)
    {
        "When conditions match"
    } else {
        "Off"
    }
}

impl Reader {
    pub(in crate::app) fn prepare_profile_name(&mut self, mode: Name) {
        let (limit, text) = match mode {
            Name::OpeningPath(index) => (
                PATH_LIMIT,
                self.profiles
                    .get(index)
                    .and_then(Profile::opening_conditions)
                    .and_then(|rules| rules.path.as_deref()),
            ),
            Name::Rename(index) => (MAX_NAME, self.profiles.get(index).map(Profile::name)),
            _ => (MAX_NAME, None),
        };
        self.editor = Editor::new(limit);
        if let Some(text) = text {
            self.editor.text = text.chars().collect();
            self.editor.caret = self.editor.text.len();
        }
    }

    pub(super) fn opening_rule_items(&self, index: usize, page: Page) -> Vec<Item> {
        let Some(profile) = self.profiles.get(index) else {
            return Vec::new();
        };
        let empty = Conditions::default();
        let rules = profile.opening_conditions().unwrap_or(&empty);
        let action =
            |title: &str, command| Item::new(title, Action::OpeningProfileRule(index, command));
        match page {
            Page::Overview => {
                let mut items = vec![action("Always apply", Command::Always)];
                if !profile.applies_on_open() {
                    items.extend([
                        Item::new(
                            "Opening orientation",
                            Action::Show(Screen::ReadingProfileOpening(index, Page::Orientation)),
                        )
                        .detail(selected(rules.orientations.len())),
                        Item::new(
                            "File path contains",
                            Action::Show(Screen::ProfileName(Name::OpeningPath(index))),
                        )
                        .detail(rules.path.as_deref().unwrap_or("Off")),
                        Item::new(
                            "Book is in a collection",
                            Action::Show(Screen::ReadingProfileOpening(index, Page::Collections)),
                        )
                        .detail(selected(rules.collections.len())),
                        action("Only for new books", Command::NewOnly),
                    ]);
                    if profile.opening_conditions().is_some() {
                        items.push(action("Clear conditions", Command::Clear));
                    }
                }
                items
            }
            Page::Orientation => ORIENTATIONS
                .into_iter()
                .map(|orientation| action(orientation.label(), Command::Orientation(orientation)))
                .collect(),
            Page::Collections => {
                let mut names: Vec<_> = self
                    .library
                    .collections()
                    .iter()
                    .map(|collection| collection.name.as_str())
                    .collect();
                names.extend(
                    rules
                        .collections
                        .iter()
                        .map(String::as_str)
                        .filter(|name| self.library.collection(name).is_none()),
                );
                names.sort_by_cached_key(|name| name.to_lowercase());
                names
                    .into_iter()
                    .map(|name| {
                        let item = action(name, Command::Collection(name.to_owned()));
                        if self.library.collection(name).is_none() {
                            item.detail("Unavailable")
                        } else {
                            item
                        }
                    })
                    .collect()
            }
        }
    }

    pub(in crate::app) fn opening_rule_checked(
        &self,
        index: usize,
        command: &Command,
    ) -> Option<bool> {
        let profile = self.profiles.get(index)?;
        let rules = profile.opening_conditions();
        Some(match command {
            Command::Always => profile.applies_on_open(),
            Command::Orientation(orientation) => {
                rules.is_some_and(|rules| rules.orientations.contains(orientation))
            }
            Command::Collection(name) => {
                rules.is_some_and(|rules| collection_index(rules, name).is_some())
            }
            Command::NewOnly => rules.is_some_and(|rules| rules.new_only),
            Command::Clear => return None,
        })
    }

    pub(in crate::app) fn change_opening_rule(
        &mut self,
        index: usize,
        command: Command,
    ) -> Result<()> {
        let mut candidate = self.profiles.clone();
        let profile = candidate
            .get_mut(index)
            .ok_or("That profile is unavailable")?;
        if matches!(command, Command::Always) {
            profile.set_apply_on_open(!profile.applies_on_open());
        } else {
            let mut rules = profile.opening_conditions().cloned().unwrap_or_default();
            match command {
                Command::Orientation(orientation) => {
                    if let Some(index) = rules
                        .orientations
                        .iter()
                        .position(|&mode| mode == orientation)
                    {
                        rules.orientations.remove(index);
                    } else {
                        rules.orientations.push(orientation);
                    }
                }
                Command::Collection(name) => {
                    if let Some(index) = collection_index(&rules, &name) {
                        rules.collections.remove(index);
                    } else if let Some(collection) = self.library.collection(&name) {
                        rules.collections.push(collection.name.clone());
                    } else {
                        return Err("This collection is unavailable".into());
                    }
                }
                Command::NewOnly => rules.new_only = !rules.new_only,
                Command::Clear => rules = Conditions::default(),
                Command::Always => unreachable!(),
            }
            profile.set_opening_conditions(rules);
        }
        self.commit_profiles(candidate)?;
        self.notice = None;
        Ok(())
    }

    pub(super) fn save_opening_path(&mut self, index: usize, value: &str) -> Result<()> {
        if !value.is_empty() && !valid_path(value) {
            return Err("Enter path text with a nonempty phrase between commas".into());
        }
        let mut candidate = self.profiles.clone();
        let profile = candidate
            .get_mut(index)
            .ok_or("That profile is unavailable")?;
        let mut rules = profile.opening_conditions().cloned().unwrap_or_default();
        rules.path = (!value.is_empty()).then(|| value.to_owned());
        profile.set_opening_conditions(rules);
        self.commit_profiles(candidate)?;
        self.show(Screen::ReadingProfileOpening(index, Page::Overview));
        Ok(())
    }
}

fn selected(count: usize) -> String {
    if count == 0 {
        "Off".into()
    } else {
        format!("{count} selected")
    }
}

fn collection_index(rules: &Conditions, name: &str) -> Option<usize> {
    let name = fold_search(name);
    rules
        .collections
        .iter()
        .position(|title| fold_search(title) == name)
}
