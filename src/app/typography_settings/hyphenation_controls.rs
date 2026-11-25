//! Stock-style hyphenation choices and a transactional draft for global limits.
use crate::TypographyLocale;
use crate::app::{Action, Item, Reader, Result, Screen};
use crate::hyphenation::{ALGORITHMIC_LIMITS, Dictionary, ENGLISH_LIMITS};
use crate::{HyphenationLimits, HyphenationMode, Settings};
use primitive_ui::RowAccessory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Side {
    Left,
    Right,
}

impl Side {
    pub(in crate::app) fn title(self) -> &'static str {
        match self {
            Self::Left => "Letters before the break",
            Self::Right => "Letters after the break",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) struct LimitsDraft {
    limits: HyphenationLimits,
    defaults: HyphenationLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Command {
    ToggleEnabled,
    Method(HyphenationMode),
    Trust(bool),
    UserRules(bool),
    DefaultMethod(HyphenationMode),
    DefaultTrust(bool),
    Limits(Option<HyphenationLimits>),
}

const METHODS: [(HyphenationMode, &str); 3] = [
    (HyphenationMode::Automatic, "Hyphenation dictionary"),
    (HyphenationMode::Algorithmic, "Algorithmic hyphenation"),
    (HyphenationMode::Authored, "Soft hyphens only"),
];

impl Reader {
    fn current_hyphenation(&self) -> Option<Settings> {
        self.active
            .as_ref()
            .filter(|book| !book.reading.probe.is_vertical())
            .map(|book| book.reading.config.settings)
    }

    pub(in crate::app) fn hyphenation_items(&self) -> Vec<Item> {
        let Some(settings) = self.current_hyphenation() else {
            return Vec::new();
        };
        let mode = settings.hyphenation_mode();
        let enabled = mode != HyphenationMode::Off;
        let language = self
            .active
            .as_ref()
            .and_then(|book| book.reading.probe.main_language)
            .map(TypographyLocale::shaping_language);
        let dictionary = Dictionary::for_language(language);
        let armenian = self.active.as_ref().is_some_and(|book| {
            crate::hyphenation::is_armenian_language(book.reading.probe.main_language)
        });
        let defaults = if settings.algorithmic_hyphenation {
            ALGORITHMIC_LIMITS
        } else if armenian {
            crate::HyphenationLimits { left: 1, right: 2 }
        } else {
            dictionary.map_or(ENGLISH_LIMITS, Dictionary::limits)
        };
        let limits = settings.hyphenation_limits.unwrap_or(defaults);
        let draft = LimitsDraft { limits, defaults };
        let optional = |action| if enabled { action } else { Action::Heading };
        let mut items = vec![
            Item::new(
                "Enable hyphenation",
                Action::Hyphenation(Command::ToggleEnabled),
            ),
            Item::new(
                "Hyphenation limits",
                optional(Action::Show(Screen::HyphenationLimits(draft))),
            )
            .detail(if settings.hyphenation_limits.is_some() {
                format!("{} before · {} after", limits.left, limits.right)
            } else {
                format!("Language defaults · {} / {}", limits.left, limits.right)
            }),
            Item::new(
                "Trust soft hyphens",
                if enabled && mode != HyphenationMode::Authored {
                    Action::Hyphenation(Command::Trust(!settings.trust_soft_hyphens))
                } else {
                    Action::Heading
                },
            )
            .detail(if mode == HyphenationMode::Authored {
                "Always on in Soft hyphens only"
            } else {
                "Prefer publisher breaks when they fit"
            }),
        ];
        items.push(Item::new(
            "Custom hyphenation rules",
            optional(Action::Hyphenation(Command::UserRules(
                !self.user_hyphenation.enabled,
            ))),
        ));
        items.extend(METHODS.map(|(method, title)| {
            let item = Item::new(
                title,
                optional(Action::Hyphenation(Command::Method(method))),
            );
            if method == HyphenationMode::Automatic {
                let name = if armenian {
                    match &self.user_hyphenation.armenian {
                        crate::user_hyphenation::ArmenianPatterns::Loaded(_) => {
                            "Armenian patterns loaded"
                        }
                        crate::user_hyphenation::ArmenianPatterns::Missing => {
                            "Armenian pattern file missing"
                        }
                        crate::user_hyphenation::ArmenianPatterns::Invalid => {
                            "Armenian pattern file invalid"
                        }
                    }
                } else {
                    dictionary.map_or("No dictionary for the main language", Dictionary::label)
                };
                if mode == method {
                    item.detail(format!("Selected · {name}"))
                } else {
                    item.detail(name)
                }
            } else if mode == method {
                item.detail("Selected")
            } else {
                item
            }
        }));
        if armenian {
            items.push(
                Item::new("Armenian pattern file", Action::Heading)
                    .detail("Copy hyph-hy.tex to reader/data/hyphenation, then reopen Reader"),
            );
        }
        let defaults = self
            .controls
            .hyphenation_default
            .unwrap_or(HyphenationMode::Automatic);
        items.push(
            Item::new(
                "Defaults for new books",
                Action::Show(Screen::HyphenationDefaults),
            )
            .detail(defaults.label()),
        );
        items
    }

    pub(in crate::app) fn hyphenation_defaults_items(&self) -> Vec<Item> {
        let current = self
            .controls
            .hyphenation_default
            .unwrap_or(HyphenationMode::Automatic);
        let trust = self.controls.trust_soft_hyphens_default.unwrap_or(false);
        let mut items = Vec::from(METHODS.map(|(mode, label)| {
            let item = Item::new(label, Action::Hyphenation(Command::DefaultMethod(mode)));
            if mode == current {
                item.detail("Selected")
            } else {
                item
            }
        }));
        let off = Item::new(
            "Off",
            Action::Hyphenation(Command::DefaultMethod(HyphenationMode::Off)),
        );
        items.push(if current == HyphenationMode::Off {
            off.detail("Selected")
        } else {
            off
        });
        items.push(
            Item::new(
                "Trust soft hyphens",
                Action::Hyphenation(Command::DefaultTrust(!trust)),
            )
            .detail("Apply when opening a new book"),
        );
        items
    }

    pub(in crate::app) fn hyphenation_limit_items(&self, draft: LimitsDraft) -> Vec<Item> {
        vec![
            Item::new(
                Side::Left.title(),
                Action::Show(Screen::HyphenationLimitValue(draft, Side::Left)),
            )
            .detail(draft.limits.left.to_string()),
            Item::new(
                Side::Right.title(),
                Action::Show(Screen::HyphenationLimitValue(draft, Side::Right)),
            )
            .detail(draft.limits.right.to_string()),
            Item::new(
                "Apply to all books",
                Action::Hyphenation(Command::Limits(
                    (draft.limits != draft.defaults).then_some(draft.limits),
                )),
            ),
            Item::new(
                "Use language defaults",
                Action::Hyphenation(Command::Limits(None)),
            )
            .detail(format!(
                "{} before · {} after",
                draft.defaults.left, draft.defaults.right
            )),
        ]
    }

    pub(in crate::app) fn hyphenation_limit_values(
        &self,
        draft: LimitsDraft,
        side: Side,
    ) -> Vec<Item> {
        (1..=10)
            .map(|value| {
                let mut next = draft;
                let before = match side {
                    Side::Left => std::mem::replace(&mut next.limits.left, value),
                    Side::Right => std::mem::replace(&mut next.limits.right, value),
                };
                let item = Item::new(
                    value.to_string(),
                    Action::Show(Screen::HyphenationLimits(next)),
                );
                if value == before {
                    item.detail("Selected")
                } else {
                    item
                }
            })
            .collect()
    }

    pub(in crate::app) fn hyphenation_accessory(&self, command: Command) -> RowAccessory {
        match command {
            Command::ToggleEnabled => RowAccessory::Toggle(
                self.current_hyphenation()
                    .is_some_and(|settings| !settings.disable_soft_hyphens),
            ),
            Command::Trust(next) | Command::DefaultTrust(next) | Command::UserRules(next) => {
                RowAccessory::Toggle(!next)
            }
            _ => RowAccessory::None,
        }
    }

    pub(in crate::app) fn activate_hyphenation(&mut self, command: Command) -> Result<()> {
        match command {
            Command::UserRules(enabled) => self.set_user_hyphenation(enabled)?,
            Command::DefaultMethod(mode) => {
                self.activate(Action::SetReadingControls(
                    crate::reading_controls::ReadingControls {
                        hyphenation_default: Some(mode),
                        ..self.controls
                    },
                ))?;
            }
            Command::DefaultTrust(trust) => {
                self.activate(Action::SetReadingControls(
                    crate::reading_controls::ReadingControls {
                        trust_soft_hyphens_default: Some(trust),
                        ..self.controls
                    },
                ))?;
            }
            Command::Limits(limits) => {
                if limits.is_some_and(|value| !value.valid()) {
                    return Err("Enter 1 to 10 letters for each hyphenation limit".into());
                }
                self.activate(Action::SetReadingControls(
                    crate::reading_controls::ReadingControls {
                        hyphenation_limits: limits,
                        ..self.controls
                    },
                ))?;
                self.show(Screen::HyphenationOptions);
            }
            Command::ToggleEnabled | Command::Method(_) | Command::Trust(_) => {
                let mut settings = self
                    .current_hyphenation()
                    .ok_or("Open a horizontal book first")?;
                match command {
                    Command::ToggleEnabled => {
                        settings.disable_soft_hyphens = !settings.disable_soft_hyphens
                    }
                    Command::Method(method) => settings = settings.with_hyphenation(method),
                    Command::Trust(trust) => settings.trust_soft_hyphens = trust,
                    _ => unreachable!(),
                }
                let mut preferences = self.current_text_preferences()?;
                preferences.record_hyphenation(settings);
                self.apply_text_profile(preferences)?;
                self.show(Screen::HyphenationOptions);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
