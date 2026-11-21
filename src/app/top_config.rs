//! Actions available from the reading view's grouped top menu.


use super::{Action, BookList, Item, Reader, Screen, text_items};
use crate::reading_controls::ReadingControls;

pub(super) use crate::book_menus::TopGroup;

impl TopGroup {
    pub(super) const ALL: [Self; 6] = [
        Self::Navigation,
        Self::Appearance,
        Self::Settings,
        Self::Tools,
        Self::Search,
        Self::Library,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Appearance => "Appearance",
            Self::Settings => "Settings",
            Self::Tools => "Tools",
            Self::Search => "Search",
            Self::Library => "Library",
        }
    }
}

impl Reader {
    pub(super) fn top_config_items(&self, group: TopGroup) -> Vec<Item> {
        match group {
            TopGroup::Navigation => self.navigation_items(),
            TopGroup::Appearance if self.fixed.is_some() => self.fixed_display_items(),
            TopGroup::Appearance => self.text_display_items(),
            TopGroup::Settings => self.reading_settings_items(),
            TopGroup::Tools => self.document_tool_items(),
            TopGroup::Search => self.document_search_items(),
            TopGroup::Library => self.reading_library_items(),
        }
    }

    fn navigation_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        let bookmarks = if self.fixed.is_some() {
            items.push(Item::new("Contents", Action::Show(Screen::FixedContents)));
            Screen::FixedBookmarks
        } else if self.active.is_some() {
            items.push(Item::new("Contents", Action::Show(Screen::Contents)));
            Screen::Bookmarks
        } else {
            return items;
        };
        items.extend([
            Item::new("Bookmarks", Action::Show(bookmarks)),
            Item::new(
                if self.page_is_bookmarked() {
                    "Remove bookmark"
                } else {
                    "Add bookmark"
                },
                Action::ToggleBookmark,
            ),
            Item::new("Go to page", Action::Show(Screen::GoToPage)),
            Item::new("Book map", Action::Show(Screen::BookMap)),
            Item::new("Page browser", Action::Overview).detail("Thumbnails and skim"),
            Item::new("Auto turn", Action::Show(Screen::AutoTurn)).detail(
                self.auto_turn.interval().map_or_else(
                    || "Off".to_owned(),
                    |interval| format!("Every {} seconds", interval.as_secs()),
                ),
            ),
        ]);
        if self
            .active
            .as_ref()
            .is_some_and(|active| !active.reading.links().is_empty())
        {
            items.push(Item::new("Links on this page", Action::Show(Screen::Links)));
        }
        if !self.back.is_empty() {
            items.push(Item::new("Previous location", Action::ReturnToLocation));
        }
        if !self.forward.is_empty() {
            items.push(Item::new("Next location", Action::NextLocation));
        }
        if !self.back.is_empty() || !self.forward.is_empty() {
            items.push(Item::new(
                "Location history",
                Action::Show(Screen::LocationHistory),
            ));
        }
        if self.active.is_some() {
            items.push(
                Item::new(
                    "Footnote previews",
                    Action::SetReadingControls(ReadingControls {
                        footnote_popups: !self.controls.footnote_popups,
                        ..self.controls
                    }),
                )
                .detail(if self.controls.footnote_popups {
                    "On"
                } else {
                    "Off"
                }),
            );
            if self.controls.footnote_popups {
                items.push(
                    Item::new(
                        "Preview more links",
                        Action::SetReadingControls(ReadingControls {
                            prefer_footnotes: !self.controls.prefer_footnotes,
                            ..self.controls
                        }),
                    )
                    .detail(if self.controls.prefer_footnotes {
                        "On"
                    } else {
                        "Off"
                    }),
                );
            }
        }
        items
    }

    pub(super) fn text_display_items(&self) -> Vec<Item> {
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let reading = &active.reading;
        let settings = reading.config.settings;
        let mut items = text_items(settings, reading.config.scroll);
        if reading.probe.is_vertical() {
            items.retain(|item| match item.action {
                Action::ReadingMode(_) => false,
                Action::Text(next) => {
                    next.alignment == settings.alignment
                        && next.hyphenation_mode() == settings.hyphenation_mode()
                }
                _ => true,
            });
        }
        let current_font = self
            .library
            .state(active.id)
            .and_then(|book| book.preferences.font_file.as_deref())
            .unwrap_or("Device fonts");
        items.insert(
            0,
            Item::new("Reading font", Action::ShowFonts).detail(current_font),
        );
        items.insert(
            1,
            Item::new("Reading profiles", Action::Show(Screen::ReadingProfiles)),
        );
        items
    }

    pub(super) fn fixed_display_items(&self) -> Vec<Item> {
        let Some(fixed) = &self.fixed else {
            return Vec::new();
        };
        let reading = &fixed.reading;
        let mut items = Vec::new();
        if !reading.is_spread() {
            items.extend([
                Item::new("Fit page", Action::FitPage(false)),
                Item::new("Fit width", Action::FitPage(true)),
                Item::new("Fit height", Action::FitHeight),
            ]);
        }
        items.push(Item::new("Rotate", Action::RotatePage));
        if reading.supports_spreads() {
            let spread = reading.is_spread();
            items.push(Item::new(
                if spread { "One page" } else { "Two pages" },
                Action::ComicSpread(!spread),
            ));
        }
        if reading.supports_continuous() {
            let scroll = reading.is_continuous();
            items.push(Item::new(
                if scroll { "Page turns" } else { "Scroll" },
                Action::FixedScroll(!scroll),
            ));
        }
        if reading.supports_rtl() {
            let rtl = reading.preferences().right_to_left;
            items.push(Item::new(
                if rtl {
                    "Left to right"
                } else {
                    "Right to left"
                },
                Action::ReversePageDirection,
            ));
        }
        items
    }

    pub(super) fn reading_settings_items(&self) -> Vec<Item> {
        let current = self.controls;
        let mut items = vec![
            Item::new("Reading gestures", Action::Show(Screen::Gestures))
                .detail("Assign taps, holds and swipes"),
        ];
        items.extend(self.quick_display_items());
        // Under Primitive Launcher the pages follow the launcher's theme: no Night pages switch here.
        if !self.under_launcher() {
            items.push(
                Item::new(
                    "Night pages",
                    Action::SetReadingControls(ReadingControls {
                        night_pages: !current.night_pages,
                        ..current
                    }),
                )
                .detail(if current.night_pages {
                    "Light text on dark pages"
                } else {
                    "Dark text on light pages"
                }),
            );
        }
        items.extend([
            Item::new(
                "Page turn tap zones",
                Action::SetReadingControls(ReadingControls {
                    left_touch: !current.left_touch,
                    ..current
                }),
            )
            .detail(if current.left_touch {
                "Mirrored touch zones"
            } else {
                "Standard touch zones"
            }),
            Item::new(
                "Turn pages by tapping",
                Action::SetReadingControls(ReadingControls {
                    disable_page_taps: !current.disable_page_taps,
                    ..current
                }),
            )
            .detail(if current.disable_page_taps {
                "Off · swipe or use page buttons"
            } else {
                "On"
            }),
            Item::new(
                "Reading toolbar size",
                Action::SetReadingControls(ReadingControls {
                    large_toolbar: !current.large_toolbar,
                    ..current
                }),
            )
            .detail(if current.large_toolbar {
                "Large labels and controls"
            } else {
                "Compact labels and controls"
            }),
            Item::new(
                "Full screen refresh",
                Action::SetReadingControls(ReadingControls {
                    page_refresh: current.page_refresh.next(),
                    ..current
                }),
            )
            .detail(format!("{} · paged reading", current.page_refresh.label())),
            Item::new(
                "Open both reading bars",
                Action::SetReadingControls(ReadingControls {
                    both_toolbars: !current.both_toolbars,
                    ..current
                }),
            )
            .detail(if current.both_toolbars {
                "Top swipe opens both"
            } else {
                "Top and bottom open separately"
            }),
            Item::new(
                "Open reading menus",
                Action::SetReadingControls(ReadingControls {
                    menu_activation: current.menu_activation.next(),
                    ..current
                }),
            )
            .detail(current.menu_activation.label()),
        ]);
        if self.active.is_some() || self.fixed.is_some() {
            items.push(
                Item::new("Lock page touch", Action::LockTouch)
                    .detail("Physical page buttons, Unlock touch and Done stay available"),
            );
        }
        items
    }

    pub(super) fn scan_items(&self) -> Vec<Item> {
        let Some(fixed) = &self.fixed else {
            return Vec::new();
        };
        let reading = &fixed.reading;
        let mut items = Vec::new();
        if reading.supports_scan_contrast() {
            let higher = reading.scan_contrast();
            items.push(Item::new(
                if higher {
                    "Normal contrast"
                } else {
                    "Higher contrast"
                },
                Action::ScanContrast(!higher),
            ));
        }
        if reading.supports_crop() {
            items.extend([
                Item::new("Crop", Action::CropPage),
                Item::new("Columns", Action::ScanColumns),
            ]);
            if self.ocr_assets.is_some() && self.ocr.is_none() {
                items.push(Item::new("Recognize text", Action::RecognizeText));
            }
        }
        if reading.supports_text() {
            items.extend([
                Item::new("Read text", Action::ReadFixedText),
                Item::new("Reflow scan", Action::ReflowScan),
            ]);
        }
        items
    }

    fn document_tool_items(&self) -> Vec<Item> {
        let mut items = self.scan_items();
        if self.searchable_document() {
            items.push(Item::new(
                "Highlights and notes",
                Action::Show(Screen::Highlights),
            ));
        }
        if self.active.is_some() || self.fixed.is_some() {
            items.push(
                Item::new("Import KOReader position", Action::PreviewKoReaderPosition)
                    .detail("Preview the saved position for this book"),
            );
        }
        items.push(Item::new("Reading data", Action::Show(Screen::ReadingData)));
        items
    }

    fn searchable_document(&self) -> bool {
        self.active.is_some()
            || self
                .fixed
                .as_ref()
                .is_some_and(|document| document.reading.supports_text())
    }

    pub(super) fn preferred_dictionary_item(&self) -> Item {
        Item::new("Dictionaries", Action::ShowDictionaries).detail("Sources, order and book preferences")
    }

    fn document_search_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        if self.searchable_document() {
            items.push(Item::new("Find in document", Action::Show(Screen::Find)));
            items.push(Item::new("Translate page", Action::TranslatePage));
            let has_matches = if self.fixed.is_some() {
                !self.pdf_hits.is_empty()
            } else {
                !self.hits.is_empty()
            };
            if has_matches {
                items.push(Item::new("Search results", Action::Show(Screen::Matches)));
            }
            items.push(Item::new(
                "Search history",
                Action::Show(Screen::SearchHistory),
            ));
        }
        if self.active.is_some() || self.fixed.is_some() {
            items.push(Item::new(
                "Find bookmarks",
                Action::Show(Screen::BookmarkFind),
            ));
        }
        items.extend([
            Item::new("Find a document", Action::Show(Screen::LibraryFind)),
            Item::new("Dictionary lookup", Action::Show(Screen::DictionaryEntry)),
            Item::new("Wikipedia", Action::Wikipedia).detail("Look up a topic"),
            Item::new("Online history", Action::Show(Screen::OnlineHistory)),
            Item::new(
                "Dictionary history",
                Action::Show(Screen::DictionaryHistory),
            ),
            Item::new("Vocabulary", Action::Show(Screen::Vocabulary)),
            self.preferred_dictionary_item(),
        ]);
        items
    }

    fn reading_library_items(&self) -> Vec<Item> {
        let mut items = vec![
            Item::new("Browse Documents", Action::Show(Screen::Library)),
            Item::new("Recently read", Action::ListBooks(BookList::Recent)),
        ];
        if self.has_previous_document() {
            items.push(Item::new(
                "Open previous document",
                Action::OpenPreviousDocument,
            ));
        }
        items.extend([
            Item::new("Favorites", Action::ListBooks(BookList::Favorites)),
            Item::new(
                "Collections and reading lists",
                Action::Show(Screen::OrganizeLibrary),
            ),
        ]);
        if self.active.is_some() || self.fixed.is_some() {
            items.push(Item::new("Book details", Action::OrganizeCurrent));
        }
        items
    }
}
