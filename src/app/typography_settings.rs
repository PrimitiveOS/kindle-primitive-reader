//! Typography language selection and independent publication language controls.
use super::{Action, Item, Reader, Result, Screen};
use crate::typography_language::{CATALOG, TypographyLanguages, TypographyLocale, label};
use serde::{Deserialize, Serialize};

pub(in crate::app) mod hyphenation_controls;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct CatalogLanguage(u8);

impl CatalogLanguage {
    fn tag(self) -> &'static str {
        CATALOG[usize::from(self.0)].tag
    }

    fn locale(self) -> TypographyLocale {
        TypographyLocale::parse(self.tag()).expect("valid typography catalog choice")
    }
}

impl TryFrom<String> for CatalogLanguage {
    type Error = &'static str;

    fn try_from(tag: String) -> std::result::Result<Self, Self::Error> {
        CATALOG
            .iter()
            .position(|entry| entry.tag.eq_ignore_ascii_case(&tag))
            .map(|index| Self(index as u8))
            .ok_or("Unknown typography language")
    }
}

impl From<CatalogLanguage> for String {
    fn from(language: CatalogLanguage) -> Self {
        language.tag().to_owned()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DefaultLanguage {
    Default(CatalogLanguage),
    Fallback(CatalogLanguage),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LanguageScope {
    Book,
    Default,
    Fallback,
}

impl LanguageScope {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Book => "Main language",
            Self::Default => "Default language",
            Self::Fallback => "Fallback language",
        }
    }
}

impl Reader {
    pub(super) fn default_typography_languages(&self) -> TypographyLanguages {
        let mut settings = TypographyLanguages {
            fallback: TypographyLocale::parse("en-US"),
            respect_embedded: self
                .controls
                .respect_embedded_languages_default
                .unwrap_or(true),
            ..TypographyLanguages::default()
        };
        match self.controls.typography_language_default {
            Some(DefaultLanguage::Default(choice)) => settings.main = Some(choice.locale()),
            Some(DefaultLanguage::Fallback(choice)) => settings.fallback = Some(choice.locale()),
            None => {}
        }
        settings
    }

    pub(super) fn typography_items(&self) -> Vec<Item> {
        let Some(book) = &self.active else {
            return Vec::new();
        };
        let probe = &book.reading.probe;
        let defaults = self.default_typography_languages();
        let mut items = vec![
            Item::new(
                "Main language",
                Action::Show(Screen::TypographyLanguages(LanguageScope::Book)),
            )
            .detail(label(probe.main_language)),
            Item::new(
                "Respect embedded language tags",
                Action::RespectEmbeddedLanguages(!probe.settings.typography.respect_embedded),
            )
            .detail("Use language tags within this book"),
            Item::new(
                "Default language",
                Action::Show(Screen::TypographyLanguages(LanguageScope::Default)),
            )
            .detail(
                defaults
                    .main
                    .map_or_else(|| "Use book language".into(), |main| label(Some(main))),
            ),
            Item::new(
                "Fallback language",
                Action::Show(Screen::TypographyLanguages(LanguageScope::Fallback)),
            )
            .detail(format!("{} for untagged books", label(defaults.fallback))),
            Item::new(
                "Respect embedded tags by default",
                Action::SetReadingControls(crate::reading_controls::ReadingControls {
                    respect_embedded_languages_default: Some(!defaults.respect_embedded),
                    ..self.controls
                }),
            )
            .detail("Apply when opening a new book"),
        ];
        if !probe.is_vertical() {
            items.push(
                Item::new("Hyphenation", Action::Show(Screen::HyphenationOptions))
                    .detail(probe.settings.hyphenation_mode().label()),
            );
            items.push(
                Item::new(
                    "Hanging punctuation",
                    Action::SetHangingPunctuation {
                        enabled: !probe.settings.hanging_punctuation,
                        for_new_books: false,
                    },
                )
                .detail("Let punctuation extend into the margin"),
            );
            items.push(
                Item::new(
                    "Hanging punctuation by default",
                    Action::SetHangingPunctuation {
                        enabled: !self.controls.hanging_punctuation_default.unwrap_or(false),
                        for_new_books: true,
                    },
                )
                .detail("Apply when opening a new book"),
            );
        }
        items
    }

    pub(super) fn typography_language_items(&self, scope: LanguageScope) -> Vec<Item> {
        let choice = match scope {
            LanguageScope::Book => self
                .active
                .as_ref()
                .and_then(|book| book.reading.probe.main_language),
            LanguageScope::Default => self.default_typography_languages().main,
            LanguageScope::Fallback => self.default_typography_languages().fallback,
        };
        let first = match scope {
            LanguageScope::Book => {
                let publication = self
                    .active
                    .as_ref()
                    .and_then(|book| book.reading.probe.publication_language);
                let action = if publication.is_some() {
                    Action::TypographyLanguage(scope, None)
                } else {
                    Action::Heading
                };
                Item::new("Book language", action).detail(label(publication))
            }
            LanguageScope::Default => {
                Item::new("Use book language", Action::TypographyLanguage(scope, None))
                    .detail("Clear the default for new books")
            }
            LanguageScope::Fallback => {
                Item::new("Use English (US)", Action::TypographyLanguage(scope, None))
                    .detail("Restore the built-in fallback")
            }
        };
        let mut items = vec![first];
        items.extend(CATALOG.iter().enumerate().map(|(index, entry)| {
            let selected = CatalogLanguage(index as u8);
            let detail = if choice == Some(selected.locale()) {
                format!("{} · Selected", entry.tag)
            } else {
                entry.tag.into()
            };
            Item::new(
                entry.title,
                Action::TypographyLanguage(scope, Some(selected)),
            )
            .detail(detail)
        }));
        items
    }

    pub(super) fn choose_typography_language(
        &mut self,
        scope: LanguageScope,
        choice: Option<CatalogLanguage>,
    ) -> Result<()> {
        match scope {
            LanguageScope::Book => {
                let book = self.active.as_ref().ok_or("Open a reflowable book first")?;
                let locale = choice
                    .map(CatalogLanguage::locale)
                    .or(book.reading.probe.publication_language)
                    .ok_or("This book has no publication language")?;
                let mut preferences = self.current_text_preferences()?;
                preferences.typography_language = Some(locale.as_str().to_owned());
                self.apply_text_profile(preferences)?;
            }
            LanguageScope::Default | LanguageScope::Fallback => {
                let previous = self.controls.typography_language_default;
                let next = match (scope, choice) {
                    (LanguageScope::Default, Some(language)) => {
                        Some(DefaultLanguage::Default(language))
                    }
                    (LanguageScope::Fallback, Some(language)) => {
                        Some(DefaultLanguage::Fallback(language))
                    }
                    (LanguageScope::Default, None)
                        if matches!(previous, Some(DefaultLanguage::Default(_))) =>
                    {
                        None
                    }
                    (LanguageScope::Fallback, None)
                        if matches!(previous, Some(DefaultLanguage::Fallback(_))) =>
                    {
                        None
                    }
                    _ => previous,
                };
                self.activate(Action::SetReadingControls(
                    crate::reading_controls::ReadingControls {
                        typography_language_default: next,
                        ..self.controls
                    },
                ))?;
            }
        }
        self.show(Screen::TypographyRules);
        Ok(())
    }

    pub(super) fn set_hanging_punctuation(
        &mut self,
        enabled: bool,
        for_new_books: bool,
    ) -> Result<()> {
        if for_new_books {
            return self.activate(Action::SetReadingControls(
                crate::reading_controls::ReadingControls {
                    hanging_punctuation_default: Some(enabled),
                    ..self.controls
                },
            ));
        }
        let mut preferences = self.current_text_preferences()?;
        preferences.hanging_punctuation = Some(enabled);
        self.apply_text_profile(preferences)?;
        self.show(Screen::TypographyRules);
        Ok(())
    }

    pub(super) fn set_embedded_language_respect(&mut self, respect: bool) -> Result<()> {
        let mut preferences = self.current_text_preferences()?;
        preferences.respect_embedded_languages = Some(respect);
        self.apply_text_profile(preferences)?;
        self.show(Screen::TypographyRules);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
