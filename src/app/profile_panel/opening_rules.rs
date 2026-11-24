//! Editing the conditions for applying a profile when a book opens.
use super::opening_time::Endpoint;
use super::{
    Action, Change, Editor, Item, LIST_X, LIST_Y, Name, Profile, Reader, ReadingOrientation, Rect,
    Result, Screen,
};
use crate::library::fold_search;
use crate::reading_profiles::{MAX_NAME, opening::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Page {
    Overview,
    Orientation,
    Metadata,
    Collections,
    TimeWindow,
    TimeEdit(Endpoint),
}

impl Page {
    pub(in crate::app) fn title(self) -> &'static str {
        match self {
            Self::Overview => "Apply on book opening",
            Self::Orientation => "Opening orientations",
            Self::Metadata => "Opening book metadata",
            Self::Collections => "Opening collections",
            Self::TimeWindow | Self::TimeEdit(_) => "Opening time window",
        }
    }

    pub(in crate::app) fn back(self, index: usize) -> Screen {
        match self {
            Self::Overview => Screen::ReadingProfile(index),
            Self::TimeEdit(_) => Screen::ReadingProfileOpening(index, Self::TimeWindow),
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
    ClearTimeWindow,
}

const CURRENT_BOOK: Rect = Rect::new(LIST_X, LIST_Y + 248, 340, 100);

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
    pub(in crate::app) fn current_opening_book_button(&self) -> Option<Rect> {
        let visible = match self.screen {
            Screen::ProfileName(Name::OpeningPath(_)) => self.open_book_id().is_some(),
            Screen::ProfileName(Name::OpeningMetadata(..)) => {
                self.opening_metadata_phrase.is_some()
            }
            _ => false,
        };
        visible.then_some(CURRENT_BOOK)
    }

    pub(in crate::app) fn choose_current_opening_book(&mut self, x: f32, y: f32) -> Option<Change> {
        if !self.current_opening_book_button()?.contains(x, y) {
            return None;
        }
        if matches!(self.screen, Screen::ProfileName(Name::OpeningMetadata(..))) {
            let phrase = self.opening_metadata_phrase.as_ref()?;
            self.editor.text = phrase.chars().collect();
            self.editor.caret = self.editor.text.len();
            self.notice = None;
            return Some(Change::Partial);
        }
        let path = self
            .active
            .as_ref()
            .map(|book| book.entry.library_path())
            .or_else(|| self.fixed.as_ref().map(|book| book.entry.library_path()));
        let Some(path) = path.and_then(std::path::Path::to_str).filter(|value| {
            !value.contains(',') && !path::contains_shortcut(value) && valid_path(value)
        }) else {
            return Some(self.report("Use a distinctive part of the current book's path"));
        };
        self.editor.text = path.chars().collect();
        self.editor.caret = self.editor.text.len();
        self.notice = None;
        Some(Change::Partial)
    }

    pub(in crate::app) fn prepare_profile_name(&mut self, mode: Name) {
        self.opening_metadata_phrase = match mode {
            Name::OpeningMetadata(_, field) => self
                .current_opening_metadata()
                .and_then(|metadata| metadata.current_phrase(field)),
            _ => None,
        };
        let (limit, text) = match mode {
            Name::OpeningPath(index) => (
                PATH_LIMIT,
                self.profiles
                    .get(index)
                    .and_then(Profile::opening_conditions)
                    .and_then(|rules| rules.path.as_deref()),
            ),
            Name::OpeningMetadata(index, field) => (
                METADATA_LIMIT,
                self.profiles
                    .get(index)
                    .and_then(Profile::opening_conditions)
                    .and_then(|rules| rules.metadata.get(&field))
                    .map(String::as_str),
            ),
            Name::OpeningTime(..) => (2, None),
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
                            if self.negotiated_opening_profiles {
                                "Opening screen orientation"
                            } else {
                                "Preferred opening orientation"
                            },
                            Action::Show(Screen::ReadingProfileOpening(index, Page::Orientation)),
                        )
                        .detail(selected(rules.orientations.len())),
                        Item::new(
                            "Book metadata contains",
                            Action::Show(Screen::ReadingProfileOpening(index, Page::Metadata)),
                        )
                        .detail(selected(rules.metadata.len())),
                        Item::new(
                            "File path contains",
                            Action::Show(Screen::ProfileName(Name::OpeningPath(index))),
                        )
                        .detail(rules.path.as_deref().map_or_else(
                            || "Off".into(),
                            |pattern| self.opening_rule_path_detail(pattern),
                        )),
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
                items.push(
                    Item::new(
                        "Only within time window",
                        Action::Show(Screen::ReadingProfileOpening(index, Page::TimeWindow)),
                    )
                    .detail(
                        profile
                            .opening_time()
                            .map_or_else(|| "Any time".into(), super::opening_time::window_label),
                    ),
                );
                items
            }
            Page::Orientation => ORIENTATIONS
                .into_iter()
                .map(|orientation| action(orientation.label(), Command::Orientation(orientation)))
                .collect(),
            Page::Metadata => MetadataField::ALL
                .into_iter()
                .map(|field| {
                    Item::new(
                        field.label(),
                        Action::Show(Screen::ProfileName(Name::OpeningMetadata(index, field))),
                    )
                    .detail(rules.metadata.get(&field).map_or("Off", String::as_str))
                })
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
            Page::TimeWindow | Page::TimeEdit(_) => self.opening_time_items(index),
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
            Command::Clear | Command::ClearTimeWindow => return None,
        })
    }

    pub(in crate::app) fn change_opening_rule(
        &mut self,
        index: usize,
        command: Command,
    ) -> Result<()> {
        if matches!(command, Command::ClearTimeWindow) {
            return self.save_opening_time(index, None);
        }
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
                Command::Always | Command::ClearTimeWindow => unreachable!(),
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

    pub(super) fn save_opening_metadata(
        &mut self,
        index: usize,
        field: MetadataField,
        value: &str,
    ) -> Result<()> {
        if !value.is_empty() && !valid_metadata(value) {
            return Err("Enter a nonempty phrase between commas".into());
        }
        let mut candidate = self.profiles.clone();
        let profile = candidate
            .get_mut(index)
            .ok_or("That profile is unavailable")?;
        let mut rules = profile.opening_conditions().cloned().unwrap_or_default();
        if value.is_empty() {
            rules.metadata.remove(&field);
        } else {
            rules.metadata.insert(field, value.to_owned());
        }
        profile.set_opening_conditions(rules);
        self.commit_profiles(candidate)?;
        self.show(Name::OpeningMetadata(index, field).back());
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
