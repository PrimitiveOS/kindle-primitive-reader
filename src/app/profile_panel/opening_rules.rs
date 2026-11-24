//! Editing the conditions for applying profiles as books open or close.
use super::opening_time::Endpoint;
use super::{
    Action, Change, Editor, Item, LIST_X, LIST_Y, Name, Profile, Reader, ReadingOrientation, Rect,
    Result, Screen,
};
use crate::library::fold_search;
use crate::reading_profiles::{MAX_NAME, opening::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Event {
    Opening,
    Closing,
}

impl Event {
    pub(in crate::app) fn screen(self, index: usize, page: Page) -> Screen {
        match self {
            Self::Opening => Screen::ReadingProfileOpening(index, page),
            Self::Closing => Screen::ReadingProfileClosing(index, page),
        }
    }

    pub(super) fn action(self, index: usize, command: Command) -> Action {
        match self {
            Self::Opening => Action::OpeningProfileRule(index, command),
            Self::Closing => Action::ClosingProfileRule(index, command),
        }
    }

    fn path_name(self, index: usize) -> Name {
        match self {
            Self::Opening => Name::OpeningPath(index),
            Self::Closing => Name::ClosingPath(index),
        }
    }

    fn metadata_name(self, index: usize, field: MetadataField) -> Name {
        match self {
            Self::Opening => Name::OpeningMetadata(index, field),
            Self::Closing => Name::ClosingMetadata(index, field),
        }
    }

    fn conditions(self, profile: &Profile) -> Option<&Conditions> {
        match self {
            Self::Opening => profile.opening_conditions(),
            Self::Closing => profile.closing_options().map(|options| &options.conditions),
        }
    }

    fn set_conditions(self, profile: &mut Profile, conditions: Conditions) {
        match self {
            Self::Opening => profile.set_opening_conditions(conditions),
            Self::Closing => {
                let mut options = profile.closing_options().cloned().unwrap_or_default();
                options.conditions = conditions;
                profile.set_closing_options(options);
            }
        }
    }

    pub(super) fn time(self, profile: &Profile) -> Option<TimeWindow> {
        match self {
            Self::Opening => profile.opening_time(),
            Self::Closing => profile.closing_options()?.time,
        }
    }

    pub(super) fn set_time(self, profile: &mut Profile, time: Option<TimeWindow>) {
        match self {
            Self::Opening => profile.set_opening_time(time),
            Self::Closing => {
                let mut options = profile.closing_options().cloned().unwrap_or_default();
                options.time = time;
                profile.set_closing_options(options);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Page {
    Overview,
    Conditions,
    Orientation,
    Metadata,
    Collections,
    TimeWindow,
    TimeEdit(Endpoint),
    Delay,
    CustomDelay,
}

impl Page {
    pub(in crate::app) fn title(self, event: Event) -> &'static str {
        match (event, self) {
            (Event::Opening, Self::Overview) => "Apply on book opening",
            (Event::Closing, Self::Overview) => "Apply on book closing",
            (_, Self::Conditions) => "Closing conditions",
            (Event::Opening, Self::Orientation) => "Opening orientations",
            (Event::Closing, Self::Orientation) => "Closing orientations",
            (Event::Opening, Self::Metadata) => "Opening book metadata",
            (Event::Closing, Self::Metadata) => "Closing book metadata",
            (Event::Opening, Self::Collections) => "Opening collections",
            (Event::Closing, Self::Collections) => "Closing collections",
            (Event::Opening, Self::TimeWindow | Self::TimeEdit(_)) => "Opening time window",
            (Event::Closing, Self::TimeWindow | Self::TimeEdit(_)) => "Closing time window",
            (_, Self::Delay | Self::CustomDelay) => "Execution delay",
        }
    }

    pub(in crate::app) fn back(self, event: Event, index: usize) -> Screen {
        match self {
            Self::Overview => Screen::ReadingProfile(index),
            Self::TimeEdit(_) => event.screen(index, Self::TimeWindow),
            Self::CustomDelay => event.screen(index, Self::Delay),
            Self::Conditions | Self::TimeWindow => event.screen(index, Self::Overview),
            _ if event == Event::Closing => event.screen(index, Self::Conditions),
            _ => event.screen(index, Self::Overview),
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
    Ask,
    SetTiming(Timing),
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
            Screen::ProfileName(Name::OpeningPath(_) | Name::ClosingPath(_)) => {
                self.open_book_id().is_some()
            }
            Screen::ProfileName(Name::OpeningMetadata(..) | Name::ClosingMetadata(..)) => {
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
        if matches!(
            self.screen,
            Screen::ProfileName(Name::OpeningMetadata(..) | Name::ClosingMetadata(..))
        ) {
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
        let event = mode.rule_event();
        self.opening_metadata_phrase = match mode {
            Name::OpeningMetadata(_, field) | Name::ClosingMetadata(_, field) => self
                .current_opening_metadata()
                .and_then(|metadata| metadata.current_phrase(field)),
            _ => None,
        };
        let (limit, text) = match mode {
            Name::OpeningPath(index) | Name::ClosingPath(index) => (
                PATH_LIMIT,
                self.profiles
                    .get(index)
                    .and_then(|profile| event.conditions(profile))
                    .and_then(|rules| rules.path.as_deref()),
            ),
            Name::OpeningMetadata(index, field) | Name::ClosingMetadata(index, field) => (
                METADATA_LIMIT,
                self.profiles
                    .get(index)
                    .and_then(|profile| event.conditions(profile))
                    .and_then(|rules| rules.metadata.get(&field))
                    .map(String::as_str),
            ),
            Name::OpeningTime(..) | Name::ClosingTime(..) => (2, None),
            Name::Rename(index) => (MAX_NAME, self.profiles.get(index).map(Profile::name)),
            _ => (MAX_NAME, None),
        };
        self.editor = Editor::new(limit);
        if let Some(text) = text {
            self.editor.text = text.chars().collect();
            self.editor.caret = self.editor.text.len();
        }
    }

    pub(super) fn profile_rule_items(&self, event: Event, index: usize, page: Page) -> Vec<Item> {
        let Some(profile) = self.profiles.get(index) else {
            return Vec::new();
        };
        let empty = Conditions::default();
        let rules = event.conditions(profile).unwrap_or(&empty);
        let action = |title: &str, command| Item::new(title, event.action(index, command));
        match page {
            Page::Overview if event == Event::Closing => self.close_profile_items(index),
            Page::Overview => {
                let mut items = vec![action("Always apply", Command::Always)];
                if !profile.applies_on_open() {
                    items.extend(self.condition_rows(event, index, rules));
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
                items.push(action("Ask to execute", Command::Ask));
                items.push(
                    Item::new(
                        "Execution delay",
                        Action::Show(Screen::ReadingProfileOpening(index, Page::Delay)),
                    )
                    .detail(profile.opening_execution().map_or_else(
                        || "At opening".into(),
                        |options| super::opening_execution::timing_label(options.timing),
                    )),
                );
                items
            }
            Page::Conditions => self.condition_rows(event, index, rules),
            Page::Orientation => ORIENTATIONS
                .into_iter()
                .filter(|mode| {
                    event == Event::Opening || *mode != ReadingOrientation::FollowLauncher
                })
                .map(|orientation| action(orientation.label(), Command::Orientation(orientation)))
                .collect(),
            Page::Metadata => MetadataField::ALL
                .into_iter()
                .map(|field| {
                    Item::new(
                        field.label(),
                        Action::Show(Screen::ProfileName(event.metadata_name(index, field))),
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
            Page::TimeWindow | Page::TimeEdit(_) => self.profile_time_items(event, index),
            Page::Delay | Page::CustomDelay => self.opening_delay_items(index),
        }
    }

    fn condition_rows(&self, event: Event, index: usize, rules: &Conditions) -> Vec<Item> {
        let command = |text, command| Item::new(text, event.action(index, command));
        let mut items = vec![
            Item::new(
                match event {
                    Event::Closing => "Closing screen orientation",
                    Event::Opening if self.negotiated_opening_profiles => {
                        "Opening screen orientation"
                    }
                    Event::Opening => "Preferred opening orientation",
                },
                Action::Show(event.screen(index, Page::Orientation)),
            )
            .detail(selected(rules.orientations.len())),
            Item::new(
                "Book metadata contains",
                Action::Show(event.screen(index, Page::Metadata)),
            )
            .detail(selected(rules.metadata.len())),
            Item::new(
                "File path contains",
                Action::Show(Screen::ProfileName(event.path_name(index))),
            )
            .detail(rules.path.as_deref().map_or_else(
                || "Off".into(),
                |pattern| self.opening_rule_path_detail(pattern),
            )),
            Item::new(
                "Book is in a collection",
                Action::Show(event.screen(index, Page::Collections)),
            )
            .detail(selected(rules.collections.len())),
        ];
        if event == Event::Opening {
            items.push(command("Only for new books", Command::NewOnly));
        }
        if rules.has_matchers() || rules.new_only {
            items.push(command("Clear conditions", Command::Clear));
        }
        items
    }

    pub(in crate::app) fn profile_rule_checked(
        &self,
        event: Event,
        index: usize,
        command: &Command,
    ) -> Option<bool> {
        let profile = self.profiles.get(index)?;
        let rules = event.conditions(profile);
        Some(match command {
            Command::Always => profile.applies_on_open(),
            Command::Orientation(orientation) => {
                rules.is_some_and(|rules| rules.orientations.contains(orientation))
            }
            Command::Collection(name) => {
                rules.is_some_and(|rules| collection_index(rules, name).is_some())
            }
            Command::NewOnly => rules.is_some_and(|rules| rules.new_only),
            Command::Ask => profile
                .opening_execution()
                .is_some_and(|options| options.ask),
            Command::Clear | Command::ClearTimeWindow | Command::SetTiming(_) => return None,
        })
    }

    pub(in crate::app) fn change_opening_rule(
        &mut self,
        index: usize,
        command: Command,
    ) -> Result<()> {
        self.change_profile_rule(Event::Opening, index, command)
    }

    pub(in crate::app) fn change_profile_rule(
        &mut self,
        event: Event,
        index: usize,
        command: Command,
    ) -> Result<()> {
        if event == Event::Closing
            && matches!(
                command,
                Command::Always
                    | Command::NewOnly
                    | Command::Ask
                    | Command::SetTiming(_)
                    | Command::Orientation(ReadingOrientation::FollowLauncher)
            )
        {
            return Err("This setting is unavailable on book closing".into());
        }
        if matches!(command, Command::ClearTimeWindow) {
            return self.save_profile_time(event, index, None);
        }
        let mut candidate = self.profiles.clone();
        let profile = candidate
            .get_mut(index)
            .ok_or("That profile is unavailable")?;
        if matches!(command, Command::Ask | Command::SetTiming(_)) {
            let mut options = profile.opening_execution().unwrap_or_default();
            match command {
                Command::Ask => options.ask = !options.ask,
                Command::SetTiming(timing) => options.timing = timing,
                _ => unreachable!(),
            }
            profile.set_opening_execution(options);
        } else if matches!(command, Command::Always) {
            profile.set_apply_on_open(!profile.applies_on_open());
        } else {
            let mut rules = event.conditions(profile).cloned().unwrap_or_default();
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
                Command::Always
                | Command::ClearTimeWindow
                | Command::Ask
                | Command::SetTiming(_) => unreachable!(),
            }
            event.set_conditions(profile, rules);
        }
        self.commit_profiles(candidate)?;
        self.notice = None;
        Ok(())
    }

    pub(super) fn save_profile_path(
        &mut self,
        event: Event,
        index: usize,
        value: &str,
    ) -> Result<()> {
        if !value.is_empty() && !valid_path(value) {
            return Err("Enter path text with a nonempty phrase between commas".into());
        }
        let mut candidate = self.profiles.clone();
        let profile = candidate
            .get_mut(index)
            .ok_or("That profile is unavailable")?;
        let mut rules = event.conditions(profile).cloned().unwrap_or_default();
        rules.path = (!value.is_empty()).then(|| value.to_owned());
        event.set_conditions(profile, rules);
        self.commit_profiles(candidate)?;
        self.show(event.path_name(index).back());
        Ok(())
    }

    pub(super) fn save_profile_metadata(
        &mut self,
        event: Event,
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
        let mut rules = event.conditions(profile).cloned().unwrap_or_default();
        if value.is_empty() {
            rules.metadata.remove(&field);
        } else {
            rules.metadata.insert(field, value.to_owned());
        }
        event.set_conditions(profile, rules);
        self.commit_profiles(candidate)?;
        self.show(event.metadata_name(index, field).back());
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
