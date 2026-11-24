//! Actions available from the reading view's grouped top menu.

use super::{Action, BookList, Item, Reader, Screen, chapter_navigation::Direction, text_items};
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
            TopGroup::Settings => {
                let mut items = self.reading_settings_items();
                items.insert(
                    0,
                    Item::new("Highlight style", Action::HighlightStyle)
                        .detail(self.effective_highlight_style().label()),
                );
                items
            }
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
            Item::new("Contents settings", Action::Show(Screen::ContentsSettings)),
            Item::new(
                if self.page_is_bookmarked() {
                    "Remove bookmark"
                } else {
                    "Add bookmark"
                },
                Action::ToggleBookmark,
            ),
            Item::new("Go to page", Action::Show(Screen::GoToPage)),
            Item::new("Previous chapter", Action::Chapter(Direction::Previous)),
            Item::new("Next chapter", Action::Chapter(Direction::Next)),
            Item::new("Book map", Action::Show(Screen::BookMap)),
            Item::new("Custom hidden flows", Action::Show(Screen::HiddenFlows)),
            Item::new("Page browser", Action::Overview).detail("Browse page thumbnails"),
            Item::new("Skim", Action::Skim).detail("Preview a position, then keep it or return"),
            Item::new(
                "Chapter navigation",
                Action::Show(Screen::ChapterNavigation),
            )
            .detail("Choose Contents levels"),
            Item::new("Auto turn", Action::Show(Screen::AutoTurn)).detail(
                self.auto_turn.interval().map_or_else(
                    || "Off".to_owned(),
                    |interval| format!("Every {} seconds", interval.as_secs()),
                ),
            ),
        ]);
        if let Some(book) = self
            .active
            .as_ref()
            .filter(|book| book.entry.format == crate::library::DocumentFormat::Epub)
        {
            if book.reading.probe.has_nonlinear_chapters()
                && !book.reading.config.hidden_flows.enabled
                && !book.reading.config.scroll
                && !book.reading.config.facing_pages
            {
                items.push(
                    Item::new("Hide non-linear fragments", Action::ToggleNonlinear).detail(
                        if book.reading.config.hide_nonlinear {
                            "On"
                        } else {
                            "Off"
                        },
                    ),
                );
                items.push(
                    Item::new(
                        "Default for non-linear fragments",
                        Action::SetReadingControls(ReadingControls {
                            hide_nonlinear_default: !self.controls.hide_nonlinear_default,
                            ..self.controls
                        }),
                    )
                    .detail(if self.controls.hide_nonlinear_default {
                        "Hidden"
                    } else {
                        "Shown"
                    }),
                );
            }
            let mode = book.numbering.as_ref().map_or_else(
                || {
                    if book.reading.probe.printed_pages.is_empty() {
                        "Screen pages".into()
                    } else {
                        "Publisher labels".into()
                    }
                },
                |map| format!("{} characters per page", map.stride),
            );
            items.push(
                Item::new("Page numbering", Action::Show(Screen::PageNumbering)).detail(mode),
            );
        }
        if let Some(book) = self.active.as_ref().filter(|book| book.page_labels() > 0) {
            let label = if book.numbering.is_some() {
                "Stable"
            } else {
                "Printed"
            };
            items.push(
                Item::new(
                    "Stable page number list",
                    Action::Show(Screen::PublisherPages),
                )
                .detail(format!(
                    "{} {} pages",
                    book.page_labels(),
                    label.to_lowercase()
                )),
            );
            items.push(
                Item::new(
                    format!("{label} pages in footer"),
                    Action::TogglePublisherFooter,
                )
                .detail(if self.publisher_footer_enabled() {
                    "On"
                } else {
                    "Off"
                }),
            );
            items.push(
                Item::new(
                    format!("{label} pages in margin"),
                    Action::TogglePublisherMargins,
                )
                .detail(if self.publisher_margins_enabled() {
                    "On"
                } else {
                    "Off"
                }),
            );
        }
        if !self.page_link_items().is_empty() {
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
        let mut items = text_items(settings, reading.config.scroll, active.id);
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
        items.insert(2, self.reading_guide_entry());
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
        items.push(self.reading_guide_entry());
        items
    }

    pub(super) fn reading_settings_items(&self) -> Vec<Item> {
        let current = self.controls;
        let mut items = vec![
            Item::new(
                "Bookmarks and notes",
                Action::Show(Screen::BookmarkListSettings),
            )
            .detail(if current.bookmark_sort_by_date {
                "By date"
            } else {
                "By page in reading order"
            }),
            Item::new(
                "Selection dialog position",
                Action::Show(Screen::SelectionPosition),
            )
            .detail(current.selection_popup_position.label()),
            Item::new(
                "Long-press on text",
                Action::Show(Screen::LongPressSettings),
            )
            .detail(if current.dictionary_on_single_word.unwrap_or(true) {
                format!(
                    "Dictionary on one word · otherwise {}",
                    current.selection_long_press_action.label()
                )
            } else {
                current.selection_long_press_action.label().to_owned()
            }),
            Item::new("Document defaults", Action::Show(Screen::DocumentDefaults))
                .detail("Settings for new books; existing books stay unchanged"),
            Item::new("Reading gestures", Action::Show(Screen::Gestures))
                .detail("Assign taps, holds and swipes"),
            Item::new(
                "Navigation settings",
                Action::Show(Screen::NavigationSettings),
            )
            .detail("Opening location and Skim position"),
            Item::new("Links", Action::Show(Screen::LinkSettings))
                .detail("Link taps and return gesture"),
            Item::new(
                "Invert page-turn buttons",
                Action::SetReadingControls(ReadingControls {
                    invert_page_turn_buttons: !current.invert_page_turn_buttons,
                    ..current
                }),
            )
            .detail(if current.invert_page_turn_buttons {
                "Previous and next buttons are swapped"
            } else {
                "Standard physical button directions"
            }),
            Item::new(
                "At the end of a book",
                Action::Show(Screen::EndActionSettings),
            )
            .detail(current.end_action.label()),
            Item::new(
                "Save reading data",
                Action::Show(Screen::MetadataSaveSettings),
            )
            .detail(current.metadata_save.label()),
        ];
        if self.active.is_some() {
            items.insert(
                1,
                Item::new("Style tweaks", Action::Show(Screen::StyleTweaks))
                    .detail("Supported user CSS and live defaults"),
            );
        }
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
            if reading.supports_semi_auto_crop() {
                items.push(Item::new("Semi-auto crop", Action::SemiAutoCrop));
            }
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
        let timer = self
            .read_alarm
            .caption(std::time::Instant::now())
            .unwrap_or_else(|| "Set an interval or alarm".into());
        items.insert(0, Item::new("Read timer", Action::ReadAlarm).detail(timer));
        items.insert(
            1,
            Item::new(
                "Reading statistics",
                Action::ReadingHistory(super::reading_history::Command::Open(self.open_book_id())),
            ),
        );
        items.push(Item::new("Save reading data", Action::SaveBookData));
        if self.searchable_document() {
            items.push(Item::new("Saved marks", Action::Show(Screen::Highlights)));
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
        Item::new("Dictionaries", Action::ShowDictionaries)
            .detail("Display, sources and book preferences")
    }

    fn document_search_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        if self.searchable_document() {
            items.push(Item::new("Find in document", Action::Show(Screen::Find)));
            items.push(Item::new(
                "Search settings",
                Action::Show(Screen::SearchSettings),
            ));
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
                "Translation settings",
                Action::Show(Screen::TranslationSettings),
            ),
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
            Item::new(
                "Bookmark browser",
                Action::BookmarkBrowser(super::bookmark_browser::Command::Open),
            ),
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
            items.insert(
                1,
                Item::new("Current book folder", Action::CurrentBookFolder),
            );
            if self.controls.folder_order.supports_adjacent_files() {
                items.splice(
                    2..2,
                    [
                        Item::new(
                            "Previous file in folder",
                            Action::OpenFolderFile(super::FolderDirection::Previous),
                        ),
                        Item::new(
                            "Next file in folder",
                            Action::OpenFolderFile(super::FolderDirection::Next),
                        ),
                    ],
                );
            }
            if self
                .current_document()
                .is_some_and(|(book, _)| book.can_delete_file())
            {
                items.push(Item::new(
                    "Move to archive",
                    Action::BookArchive(super::book_archive::Command::Open),
                ));
            }
            items.push(Item::new("Book details", Action::OrganizeCurrent));
        }
        items
    }
}
