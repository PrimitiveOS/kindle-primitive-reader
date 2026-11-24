//! Named sets of enabled offline dictionary sources.
use super::{Action, Editor, Item, Reader, Result, Screen, io};
use crate::dictionary::InstalledDictionary;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(super) const MAX_NAME: usize = 48;
const MAX_PRESETS: usize = 16;
const MAX_LABEL_BYTES: usize = 144;

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
    Unavailable(usize, Return),
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::List => "Dictionary presets",
            Self::Details(_) => "Dictionary preset",
            Self::Name(_) => "Name dictionary preset",
            Self::Confirm(_, Edit::Update) => "Update dictionary preset?",
            Self::Confirm(_, Edit::Delete) => "Delete dictionary preset?",
            Self::Unavailable(..) => "Unavailable dictionaries",
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
    Cycle,
    Edit(usize, Edit),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Return {
    Details,
    List,
    Book,
    Fixed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct Settings(BTreeMap<String, Membership>);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Membership {
    Legacy(Vec<PathBuf>),
    Named {
        paths: Vec<PathBuf>,
        labels: Vec<Option<String>>,
    },
}

impl Membership {
    fn paths(&self) -> &[PathBuf] {
        match self {
            Self::Legacy(paths) | Self::Named { paths, .. } => paths,
        }
    }

    fn label(&self, index: usize) -> Option<&str> {
        match self {
            Self::Legacy(_) => None,
            Self::Named { labels, .. } => labels.get(index)?.as_deref(),
        }
    }

    fn is_valid(&self) -> bool {
        let paths = self.paths();
        paths.len() <= crate::dictionary::MAX_DICTIONARIES
            && paths.iter().enumerate().all(|(index, path)| {
                super::dictionary_sources::valid_dictionary_key(path)
                    && !paths[..index].contains(path)
            })
            && match self {
                Self::Legacy(_) => true,
                Self::Named { labels, .. } => {
                    labels.len() == paths.len()
                        && labels
                            .iter()
                            .flatten()
                            .all(|label| display_label(label).as_deref() == Some(label.as_str()))
                }
            }
    }

    fn missing_labels(&self, installed: &[InstalledDictionary]) -> Vec<String> {
        self.paths()
            .iter()
            .enumerate()
            .filter_map(|(index, path)| {
                (!installed.iter().any(|source| &source.path == path)).then(|| {
                    self.label(index)
                        .map_or_else(|| fallback_label(path), str::to_owned)
                })
            })
            .collect()
    }

    fn remember_installed_labels(&mut self, installed: &[InstalledDictionary]) {
        let paths = self.paths();
        let labels: Vec<_> = paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                installed
                    .iter()
                    .find(|source| &source.path == path)
                    .and_then(|source| display_label(&source.title))
                    .or_else(|| self.label(index).map(str::to_owned))
            })
            .collect();
        if labels.iter().any(Option::is_some) {
            *self = Self::Named {
                paths: paths.to_vec(),
                labels,
            };
        }
    }
}

fn display_label(input: &str) -> Option<String> {
    let mut output = String::new();
    for character in input.trim().chars() {
        if character.is_control()
            || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            continue;
        }
        if output.len() + character.len_utf8() > MAX_LABEL_BYTES {
            break;
        }
        output.push(character);
    }
    let output = output.trim();
    (!output.is_empty()).then(|| output.to_owned())
}

fn fallback_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .and_then(display_label)
        .unwrap_or_else(|| "Unknown dictionary".into())
}

impl Settings {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn is_valid(&self) -> bool {
        self.0.len() <= MAX_PRESETS
            && self
                .0
                .iter()
                .all(|(name, members)| valid_name(name) && members.is_valid())
    }

    pub(super) fn names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub(super) fn get(&self, index: usize) -> io::Result<(&str, &[PathBuf])> {
        self.0
            .iter()
            .nth(index)
            .map(|(name, members)| (name.as_str(), members.paths()))
            .ok_or_else(|| io::Error::other("Dictionary preset unavailable"))
    }
}

pub(super) fn valid_name(name: &str) -> bool {
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

    fn snapshot_dictionary_members(&self) -> Membership {
        let mut members = Membership::Legacy(self.enabled_dictionary_members());
        members.remember_installed_labels(&self.installed_dictionaries);
        members
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
                if !presets.is_empty() {
                    rows.push(Item::new(
                        "Cycle through dictionary presets",
                        Action::DictionaryPreset(Command::Cycle),
                    ));
                }
                rows.extend(
                    presets
                        .0
                        .iter()
                        .enumerate()
                        .map(|(index, (name, members))| {
                            let detail = match members.paths().len() {
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
                    let mut rows = vec![
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
                    ];
                    if !presets.0[name]
                        .missing_labels(&self.installed_dictionaries)
                        .is_empty()
                    {
                        rows.insert(
                            2,
                            Item::new(
                                "Unavailable dictionaries",
                                Action::Show(Screen::DictionaryPresets(Page::Unavailable(
                                    index,
                                    Return::Details,
                                ))),
                            ),
                        );
                    }
                    rows
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
            Page::Unavailable(index, _) => presets.get(index).map_or_else(
                |_| Vec::new(),
                |(name, _)| {
                    let mut rows = vec![Item::new(name, Action::Heading)];
                    rows.extend(
                        presets.0[name]
                            .missing_labels(&self.installed_dictionaries)
                            .into_iter()
                            .map(|source| Item::new(source, Action::Heading)),
                    );
                    rows
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
            Page::Unavailable(index, origin) => match origin {
                Return::Details => Screen::DictionaryPresets(Page::Details(index)),
                Return::List => Screen::DictionaryPresets(Page::List),
                Return::Book => Screen::Book,
                Return::Fixed => Screen::Fixed,
            },
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
                let members = self.snapshot_dictionary_members();
                if members.paths().is_empty() {
                    return Err("Enable a dictionary before saving a preset".into());
                }
                (members, "Dictionary preset saved")
            }
            Name::Rename(index) => {
                let (original, _) = prefs.presets.get(index)?;
                if original == name {
                    self.back_from_dictionary_presets(Page::Name(mode));
                    return Ok(());
                }
                if next.preset_cycle_after.as_deref() == Some(original) {
                    next.preset_cycle_after = Some(name.to_owned());
                }
                let members = next.presets.0.remove(original).unwrap();
                (members, "Dictionary preset renamed")
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

    fn apply_dictionary_preset(&mut self, index: usize, origin: Return) -> Result<()> {
        self.refresh_installed_dictionaries()?;
        let mut next = self.dictionary_settings.clone();
        let (name, _) = next.presets.get(index)?;
        let name = name.to_owned();
        let members = &next.presets.0[&name];
        let has_missing = !members
            .missing_labels(&self.installed_dictionaries)
            .is_empty();
        next.disabled = self
            .installed_dictionaries
            .iter()
            .filter(|source| !members.paths().contains(&source.path))
            .map(|source| source.path.clone())
            .collect();
        if origin != Return::Details {
            next.preset_cycle_after = Some(name.clone());
        }
        self.save_source_preferences(next)?;
        if has_missing {
            self.show(Screen::DictionaryPresets(Page::Unavailable(index, origin)));
        } else if origin == Return::Details {
            self.show(Screen::DictionaryPresets(Page::Details(index)));
        }
        self.notice = Some(format!("Loaded preset · {name}"));
        Ok(())
    }

    pub(super) fn dictionary_preset_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => {
                self.refresh_installed_dictionaries()?;
                self.show(Screen::DictionaryPresets(Page::List));
                return Ok(());
            }
            Command::Apply(index) => return self.apply_dictionary_preset(index, Return::Details),
            Command::Cycle => {
                let prefs = &self.dictionary_settings;
                if prefs.presets.is_empty() {
                    self.notice = Some("No dictionary presets available".into());
                    return Ok(());
                }
                let index = prefs
                    .preset_cycle_after
                    .as_ref()
                    .and_then(|last| prefs.presets.names().position(|name| name > last.as_str()))
                    .unwrap_or(0);
                let origin = match self.screen {
                    Screen::Book => Return::Book,
                    Screen::Fixed => Return::Fixed,
                    _ => Return::List,
                };
                return self.apply_dictionary_preset(index, origin);
            }
            Command::Edit(..) => {}
        }
        let mut next = self.dictionary_settings.clone();
        let (page, message) = match command {
            Command::Edit(index, Edit::Update) => {
                self.refresh_installed_dictionaries()?;
                let members = self.snapshot_dictionary_members();
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
            Command::Open | Command::Apply(_) | Command::Cycle => unreachable!(),
        };
        self.save_source_preferences(next)?;
        self.show(Screen::DictionaryPresets(page));
        self.notice = Some(message);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
