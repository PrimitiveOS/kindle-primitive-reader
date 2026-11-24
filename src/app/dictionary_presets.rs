//! Named sets of enabled offline dictionary sources.
use super::{Action, Editor, Item, Reader, Result, Screen, io};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub(super) const MAX_NAME: usize = 48;
const MAX_PRESETS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Name {
    Create,
    Rename(usize),
}

impl Name {
    pub(super) fn back(self) -> Page {
        match self {
            Self::Create => Page::List,
            Self::Rename(index) => Page::Details(index),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    List,
    Details(usize),
    Name(Name),
    Confirm(usize, Edit),
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::List => "Dictionary presets",
            Self::Details(_) => "Dictionary preset",
            Self::Name(_) => "Name dictionary preset",
            Self::Confirm(_, Edit::Update) => "Update dictionary preset?",
            Self::Confirm(_, Edit::Delete) => "Delete dictionary preset?",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Edit {
    Update,
    Delete,
}

pub(super) enum Command {
    Open,
    Apply(usize),
    Edit(usize, Edit),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct Settings(BTreeMap<String, Vec<PathBuf>>);

impl Settings {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn is_valid(&self) -> bool {
        self.0.len() <= MAX_PRESETS
            && self.0.iter().all(|(name, paths)| {
                valid_name(name)
                    && paths.len() <= crate::dictionary::MAX_DICTIONARIES
                    && paths.iter().enumerate().all(|(index, path)| {
                        super::dictionary_sources::valid_dictionary_key(path)
                            && !paths[..index].contains(path)
                    })
            })
    }

    fn get(&self, index: usize) -> io::Result<(&str, &[PathBuf])> {
        self.0
            .iter()
            .nth(index)
            .map(|(name, paths)| (name.as_str(), paths.as_slice()))
            .ok_or_else(|| io::Error::other("Dictionary preset unavailable"))
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name == name.trim()
        && name.chars().count() <= MAX_NAME
        && !name.chars().any(char::is_control)
}

impl Reader {
    fn enabled_dictionary_members(&self) -> Vec<PathBuf> {
        let (_, disabled) = self.dictionary_settings.paths();
        let preferred = self.book_dictionaries();
        self.installed_dictionaries
            .iter()
            .filter(|source| preferred.contains(&source.path) || !disabled.contains(&source.path))
            .map(|source| source.path.clone())
            .collect()
    }

    pub(super) fn dictionary_preset_items(&self, page: Page) -> Vec<Item> {
        let presets = &self.dictionary_settings.presets;
        match page {
            Page::List => {
                let mut rows = if self.enabled_dictionary_members().is_empty() {
                    vec![Item::new(
                        "Enable a dictionary to create a preset",
                        Action::Heading,
                    )]
                } else {
                    vec![Item::new(
                        "Create preset from enabled dictionaries",
                        Action::Show(Screen::DictionaryPresets(Page::Name(Name::Create))),
                    )]
                };
                rows.extend(
                    presets
                        .0
                        .iter()
                        .enumerate()
                        .map(|(index, (name, members))| {
                            let detail = match members.len() {
                                0 => "No dictionaries".to_owned(),
                                1 => "1 dictionary".to_owned(),
                                count => format!("{count} dictionaries"),
                            };
                            Item::new(
                                name,
                                Action::Show(Screen::DictionaryPresets(Page::Details(index))),
                            )
                            .detail(detail)
                        }),
                );
                rows
            }
            Page::Details(index) => presets.get(index).map_or_else(
                |_| Vec::new(),
                |(name, _)| {
                    vec![
                        Item::new(name, Action::Heading),
                        Item::new(
                            "Apply preset",
                            Action::DictionaryPreset(Command::Apply(index)),
                        ),
                        Item::new(
                            "Update from enabled dictionaries",
                            Action::Show(Screen::DictionaryPresets(Page::Confirm(
                                index,
                                Edit::Update,
                            ))),
                        ),
                        Item::new(
                            "Rename",
                            Action::Show(Screen::DictionaryPresets(Page::Name(Name::Rename(
                                index,
                            )))),
                        ),
                        Item::new(
                            "Delete",
                            Action::Show(Screen::DictionaryPresets(Page::Confirm(
                                index,
                                Edit::Delete,
                            ))),
                        ),
                    ]
                },
            ),
            Page::Confirm(index, edit) => presets.get(index).map_or_else(
                |_| Vec::new(),
                |(name, _)| {
                    let action = match edit {
                        Edit::Update => "Update preset",
                        Edit::Delete => "Delete preset",
                    };
                    vec![
                        Item::new(name, Action::Heading),
                        Item::new(action, Action::DictionaryPreset(Command::Edit(index, edit))),
                        Item::new(
                            "Cancel",
                            Action::Show(Screen::DictionaryPresets(Page::Details(index))),
                        ),
                    ]
                },
            ),
            Page::Name(_) => Vec::new(),
        }
    }

    pub(super) fn prepare_dictionary_preset_name(&mut self, mode: Name) {
        self.editor = Editor::new(MAX_NAME);
        if let Name::Rename(index) = mode
            && let Ok((name, _)) = self.dictionary_settings.presets.get(index)
        {
            self.editor.text = name.chars().collect();
            self.editor.caret = self.editor.text.len();
        }
    }

    pub(super) fn back_from_dictionary_presets(&mut self, page: Page) {
        self.show(match page {
            Page::List => Screen::Dictionaries,
            Page::Details(_) => Screen::DictionaryPresets(Page::List),
            Page::Confirm(index, _) => Screen::DictionaryPresets(Page::Details(index)),
            Page::Name(mode) => Screen::DictionaryPresets(mode.back()),
        });
    }

    pub(super) fn save_dictionary_preset_name(&mut self, mode: Name, name: &str) -> Result<()> {
        if !valid_name(name) {
            return Err("Enter a dictionary preset name".into());
        }
        let prefs = &self.dictionary_settings;
        let mut next = prefs.clone();
        let (members, message) = match mode {
            Name::Create => {
                if next.presets.0.len() == MAX_PRESETS {
                    return Err("Reader reached its dictionary preset limit".into());
                }
                self.refresh_installed_dictionaries()?;
                let members = self.enabled_dictionary_members();
                if members.is_empty() {
                    return Err("Enable a dictionary before saving a preset".into());
                }
                (members, "Dictionary preset saved")
            }
            Name::Rename(index) => {
                let (original, members) = prefs.presets.get(index)?;
                if original == name {
                    self.back_from_dictionary_presets(Page::Name(mode));
                    return Ok(());
                }
                next.presets.0.remove(original);
                (members.to_vec(), "Dictionary preset renamed")
            }
        };
        if next.presets.0.contains_key(name) {
            return Err("A dictionary preset already uses that name".into());
        }
        next.presets.0.insert(name.to_owned(), members);
        self.save_source_preferences(next)?;
        let page = match mode {
            Name::Create => Page::List,
            Name::Rename(_) => {
                let index = self
                    .dictionary_settings
                    .presets
                    .0
                    .keys()
                    .position(|key| key == name)
                    .unwrap();
                Page::Details(index)
            }
        };
        self.show(Screen::DictionaryPresets(page));
        self.notice = Some(message.into());
        Ok(())
    }

    pub(super) fn dictionary_preset_command(&mut self, command: Command) -> Result<()> {
        if matches!(command, Command::Open) {
            self.refresh_installed_dictionaries()?;
            self.show(Screen::DictionaryPresets(Page::List));
            return Ok(());
        }
        let mut next = self.dictionary_settings.clone();
        let (page, message) = match command {
            Command::Apply(index) => {
                self.refresh_installed_dictionaries()?;
                let (_, members) = next.presets.get(index)?;
                let missing = members
                    .iter()
                    .filter(|key| {
                        !self
                            .installed_dictionaries
                            .iter()
                            .any(|source| &source.path == *key)
                    })
                    .count();
                next.disabled = self
                    .installed_dictionaries
                    .iter()
                    .filter(|source| !members.contains(&source.path))
                    .map(|source| source.path.clone())
                    .collect();
                let notice = if missing == 0 {
                    "Dictionary preset applied".into()
                } else {
                    format!("Dictionary preset applied · {missing} unavailable")
                };
                (Page::Details(index), notice)
            }
            Command::Edit(index, Edit::Update) => {
                self.refresh_installed_dictionaries()?;
                let members = self.enabled_dictionary_members();
                let (name, _) = next.presets.get(index)?;
                next.presets.0.insert(name.to_owned(), members);
                (Page::Details(index), "Dictionary preset updated".into())
            }
            Command::Edit(index, Edit::Delete) => {
                let (name, _) = next.presets.get(index)?;
                let name = name.to_owned();
                next.presets.0.remove(&name);
                (Page::List, "Dictionary preset deleted".into())
            }
            Command::Open => unreachable!(),
        };
        self.save_source_preferences(next)?;
        self.show(Screen::DictionaryPresets(page));
        self.notice = Some(message);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
