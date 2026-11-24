//! Stable page numbering from the source edition, independent of screen geometry.
use super::{
    Action, ActiveBook, Change, Item, Reader, SEARCH_DELAY, SEARCH_REFRESH, Screen, position,
};
use crate::library::{FixedTileAxis, MAX_CROP_MARGIN};
use crate::{Result, epub::Book, library::DocumentFormat, synthetic_pages::CharacterPages};
use primitive_app::with_storage;
use std::{borrow::Cow, io, path::PathBuf, time::Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NumberField {
    BookStride,
    DefaultStride,
    MarginSize,
    ContentDpi(u64),
    LogicalFont(u64),
    LineSpacing(u64),
    WordSpacing(super::word_spacing::Draft),
    AutoTurn,
    AutoTurnOverlapHorizontal,
    AutoTurnOverlapVertical,
    FixedTileZoom(FixedTileAxis),
    CropMargin,
    FooterTrackMinimumWidth,
    FooterTrackMarginWidth,
    FooterBatteryHideThreshold,
    FooterItemFontSize,
    FooterContainerHeight,
    FooterContainerBottomMargin,
    FooterCustomTextRepetitions,
}

fn parse_crop_margin(value: &str) -> Option<u8> {
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let whole: u8 = if whole.is_empty() && !fraction.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let fraction = match fraction.as_bytes() {
        [] => 0,
        [tens @ b'0'..=b'9'] => (tens - b'0') * 10,
        [tens @ b'0'..=b'9', units @ b'0'..=b'9'] => (tens - b'0') * 10 + units - b'0',
        _ => return None,
    };
    let hundredths = whole.checked_mul(100)?.checked_add(fraction)?;
    (hundredths <= MAX_CROP_MARGIN).then_some(hundredths)
}

impl NumberField {
    pub(super) fn parent(self) -> Screen {
        match self {
            Self::BookStride => Screen::PageNumbering,
            Self::DefaultStride | Self::MarginSize => Screen::NumberingDefaults,
            Self::ContentDpi(_)
            | Self::LogicalFont(_)
            | Self::LineSpacing(_)
            | Self::WordSpacing(_) => Screen::Book,
            Self::AutoTurn => Screen::AutoTurn,
            Self::AutoTurnOverlapHorizontal | Self::AutoTurnOverlapVertical => Screen::AutoTurnPan,
            Self::FixedTileZoom(_) | Self::CropMargin => Screen::Fixed,
            Self::FooterTrackMinimumWidth
            | Self::FooterTrackMarginWidth
            | Self::FooterBatteryHideThreshold
            | Self::FooterItemFontSize
            | Self::FooterContainerHeight
            | Self::FooterContainerBottomMargin
            | Self::FooterCustomTextRepetitions => Screen::FooterSettings,
        }
    }
}

pub(super) struct Scan {
    map: CharacterPages,
    pub(super) deadline: Instant,
    refresh: Instant,
}

impl ActiveBook {
    pub(super) fn page_labels(&self) -> usize {
        self.numbering
            .as_ref()
            .map_or(self.reading.probe.printed_pages.len(), |map| map.count)
    }

    pub(super) fn page_label(&self, row: usize) -> Option<Cow<'_, str>> {
        if let Some(map) = &self.numbering {
            return (row < map.count).then(|| Cow::Owned((row + 1).to_string()));
        }
        Some(Cow::Borrowed(
            &self.reading.probe.printed_pages.get(row)?.label,
        ))
    }
}

impl Reader {
    pub(super) fn numbering_path(&self, id: u64) -> PathBuf {
        self.numbering_directory.join(format!("{id:016x}.json"))
    }

    pub(super) fn numbering_items(&self) -> Vec<Item> {
        let Some(book) = &self.active else {
            return Vec::new();
        };
        let mut items = Vec::new();
        if let Some(scan) = &self.numbering_scan {
            items.push(
                Item::new("Numbering chapters", Action::Heading).detail(format!(
                    "{} of {} · {} characters per page",
                    scan.map.chapters.len(),
                    book.reading.probe.chapter_count(),
                    scan.map.stride
                )),
            );
        }
        let published = book.reading.probe.printed_pages.len();
        items.push(
            Item::new(
                if published > 0 {
                    "Publisher page numbers"
                } else {
                    "Screen page numbers"
                },
                Action::SetCharacterPages(None),
            )
            .detail(if published > 0 {
                format!("{published} labels supplied by this edition")
            } else {
                "Follow the current text layout".into()
            }),
        );
        let stride = book.numbering.as_ref().map_or(1500, |m| m.stride);
        items.push(
            Item::new(
                "Character-count pages",
                Action::SetCharacterPages(Some(stride)),
            )
            .detail(format!("{stride} characters per stable page")),
        );
        items.push(
            Item::new(
                "Characters per page",
                Action::Show(Screen::NumberingValue(NumberField::BookStride)),
            )
            .detail("Choose a value from 500 to 3,000"),
        );
        items.push(Item::new(
            "Defaults for new books",
            Action::Show(Screen::NumberingDefaults),
        ));
        items
    }

    pub(super) fn numbering_defaults_items(&self) -> Vec<Item> {
        let d = self.controls.numbering;
        let settings = |numbering| {
            Action::SetReadingControls(crate::reading_controls::ReadingControls {
                numbering,
                ..self.controls
            })
        };
        let flag = |title: &str, value, changed| {
            Item::new(title, settings(changed)).detail(if value { "On" } else { "Off" })
        };
        use crate::reading_controls::NumberingDefaults;
        let mut items = vec![
            Item::new(
                "Characters per page",
                Action::Show(Screen::NumberingValue(NumberField::DefaultStride)),
            )
            .detail(
                d.stride
                    .map_or_else(|| "Off".to_owned(), |size| size.to_string()),
            ),
        ];
        if d.stride.is_some() {
            items.push(Item::new(
                "Disable default character count",
                settings(NumberingDefaults { stride: None, ..d }),
            ));
            items.push(flag(
                "Override publisher page numbers",
                d.override_publisher,
                NumberingDefaults {
                    override_publisher: !d.override_publisher,
                    ..d
                },
            ));
        }
        items.extend([
            flag(
                "Ask about publisher numbers",
                d.prompt_publisher,
                NumberingDefaults {
                    prompt_publisher: !d.prompt_publisher,
                    ..d
                },
            ),
            flag(
                "Stable pages in footer",
                d.footer,
                NumberingDefaults {
                    footer: !d.footer,
                    ..d
                },
            ),
            flag(
                "Stable pages in margin",
                d.margins,
                NumberingDefaults {
                    margins: !d.margins,
                    ..d
                },
            ),
            Item::new(
                "Margin label size",
                Action::Show(Screen::NumberingValue(NumberField::MarginSize)),
            )
            .detail(d.label_size.clamp(8, 20).to_string()),
        ]);
        items
    }

    pub(super) fn apply_numbering_input(&mut self, field: NumberField) -> Result<()> {
        let value = self.editor.string();
        if let NumberField::FixedTileZoom(axis) = field {
            let value = value.trim();
            let (whole, fraction) = value.split_once('.').unwrap_or((value, "0"));
            let count = match (whole.parse::<u8>(), fraction.as_bytes()) {
                (Ok(whole @ 0..=8), [digit @ b'0'..=b'9']) => whole * 10 + (digit - b'0'),
                _ => 0,
            };
            return self.activate(Action::SetFixedTileZoom(axis, count));
        }
        if field == NumberField::CropMargin {
            let margin = parse_crop_margin(&value)
                .ok_or("Enter 0.00 to 1.50 inches with up to two decimal places")?;
            return self.activate(Action::SetCropMargin(margin));
        }
        if field == NumberField::AutoTurn {
            let seconds = value
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=crate::reading_controls::MAX_AUTO_TURN_SECONDS).contains(n))
                .ok_or("Enter an automatic page-turn interval from 1 to 86,400 seconds")?;
            self.set_auto_turn_seconds(seconds)?;
            self.show(field.parent());
            return Ok(());
        }
        if field == NumberField::FooterBatteryHideThreshold {
            let threshold = value
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|percent| *percent <= 100)
                .ok_or("Enter a battery threshold from 0% to 100%")?;
            let mut controls = self.controls;
            controls.footer_battery_hide_threshold = Some(threshold);
            controls.save(&self.controls_path)?;
            self.controls = controls;
            self.show(field.parent());
            return Ok(());
        }
        if field == NumberField::FooterItemFontSize {
            let size = value
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|size| (8..=36).contains(size))
                .ok_or("Enter a footer text size from 8 to 36")?;
            let mut controls = self.controls;
            controls.footer_item_font_size = Some(size);
            controls.save(&self.controls_path)?;
            self.controls = controls;
            self.show(field.parent());
            return Ok(());
        }
        if field == NumberField::FooterCustomTextRepetitions {
            let repetitions = value
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|number| {
                    (1..=crate::app::footer_custom_text::MAX_REPETITIONS).contains(number)
                })
                .ok_or("Enter a repetition count from 1 to 99")?;
            let mut preference = self.footer_custom_text.clone();
            preference.set_repetitions(repetitions)?;
            crate::app::footer_custom_text::save(&self.footer_custom_text_path, &preference)?;
            self.footer_custom_text = preference;
            self.show(Screen::FooterSettings);
            return Ok(());
        }
        if matches!(
            field,
            NumberField::FooterContainerHeight | NumberField::FooterContainerBottomMargin
        ) {
            let number = value.trim().parse::<u8>().ok();
            let mut controls = self.controls;
            if field == NumberField::FooterContainerHeight {
                controls.footer_container_height = Some(
                    number
                        .filter(|height| (7..=98).contains(height))
                        .ok_or("Enter a footer item height from 7 to 98")?,
                );
            } else {
                controls.footer_container_bottom_margin = Some(
                    number
                        .filter(|margin| *margin <= 49)
                        .ok_or("Enter a footer bottom margin from 0 to 49")?,
                );
            }
            controls.save(&self.controls_path)?;
            self.controls = controls;
            self.show(field.parent());
            return Ok(());
        }
        if matches!(
            field,
            NumberField::FooterTrackMinimumWidth | NumberField::FooterTrackMarginWidth
        ) {
            let number = value.trim().parse::<u8>().ok();
            let number = if field == NumberField::FooterTrackMinimumWidth {
                number
                    .filter(|width| (5..=90).contains(width) && width % 5 == 0)
                    .ok_or("Enter a minimum width from 5% to 90% in 5% steps")?
            } else {
                number
                    .filter(|width| *width <= 140)
                    .ok_or("Enter a progress bar margin from 0 to 140 pixels")?
            };
            let mut controls = self.controls;
            if field == NumberField::FooterTrackMinimumWidth {
                controls.footer_track_min_width_pct = Some(number);
            } else {
                controls.footer_track_margin_width = Some(number);
            }
            controls.save(&self.controls_path)?;
            self.controls = controls;
            self.show(field.parent());
            return Ok(());
        }
        if matches!(
            field,
            NumberField::AutoTurnOverlapHorizontal | NumberField::AutoTurnOverlapVertical
        ) {
            let overlap = value
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|value| *value <= 90)
                .ok_or("Enter a zoom overlap from 0% to 90%")?;
            let horizontal = field == NumberField::AutoTurnOverlapHorizontal;
            self.set_fixed_auto_turn_overlap(horizontal, overlap)?;
            self.show(field.parent());
            return Ok(());
        }
        let number = value.trim().parse::<u16>().ok();
        let mut controls = self.controls;
        match field {
            NumberField::ContentDpi(owner) => return self.apply_content_dpi(owner),
            NumberField::LogicalFont(owner) => return self.apply_logical_font_input(owner),
            NumberField::LineSpacing(owner) => return self.apply_line_spacing_input(owner),
            NumberField::WordSpacing(draft) => return self.apply_word_spacing_input(draft),
            NumberField::AutoTurn => unreachable!("handled above"),
            NumberField::FixedTileZoom(_) | NumberField::CropMargin => {
                unreachable!("handled above")
            }
            NumberField::AutoTurnOverlapHorizontal | NumberField::AutoTurnOverlapVertical => {
                unreachable!("handled above")
            }
            NumberField::FooterTrackMinimumWidth => unreachable!("handled above"),
            NumberField::FooterTrackMarginWidth => unreachable!("handled above"),
            NumberField::FooterBatteryHideThreshold => unreachable!("handled above"),
            NumberField::FooterItemFontSize => unreachable!("handled above"),
            NumberField::FooterContainerHeight => unreachable!("handled above"),
            NumberField::FooterContainerBottomMargin => unreachable!("handled above"),
            NumberField::FooterCustomTextRepetitions => unreachable!("handled above"),
            NumberField::MarginSize => {
                controls.numbering.label_size = number
                    .filter(|n| (8..=20).contains(n))
                    .ok_or("Enter a size from 8 to 20")?
                    as u8;
                self.publisher_label_canvas.borrow_mut().take();
            }
            NumberField::BookStride | NumberField::DefaultStride => {
                let stride = number
                    .filter(|n| (500..=3000).contains(n))
                    .ok_or("Enter 500 to 3,000 characters")?;
                if field == NumberField::BookStride {
                    return self.set_character_pages(Some(stride));
                }
                controls.numbering.stride = Some(stride);
            }
        }
        controls.save(&self.controls_path)?;
        self.controls = controls;
        self.show(field.parent());
        Ok(())
    }

    pub(super) fn apply_new_numbering_defaults(&mut self) -> bool {
        let Some(active) = self.active.as_ref() else {
            return false;
        };
        if active.entry.format != DocumentFormat::Epub
            || self
                .library
                .state(active.id)
                .is_some_and(|s| s.position.is_some())
        {
            return false;
        }
        let defaults = self.controls.numbering;
        let publisher = !active.reading.probe.printed_pages.is_empty();
        let state = self
            .library
            .remember(&active.entry, active.id, &active.reading.probe.title);
        state.publisher_page_footer = defaults.footer;
        state.publisher_page_margins = defaults.margins;
        if !publisher || defaults.override_publisher {
            state.character_page_stride = defaults.stride.filter(|n| (500..=3000).contains(n));
        }
        publisher && defaults.prompt_publisher && state.character_page_stride.is_none()
    }

    pub(super) fn publisher_offer_items(&self) -> Vec<Item> {
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let count = active.reading.probe.printed_pages.len();
        let mut items = vec![Item::new(
            format!("This edition supplies {count} page numbers"),
            Action::Heading,
        )];
        if let Some(source) = &active.reading.probe.printed_source {
            items.push(Item::new("Print edition source", Action::Heading).detail(source));
        }
        if !self.publisher_footer_enabled() || !self.publisher_margins_enabled() {
            items.push(Item::new(
                "Show numbers in footer and margin",
                Action::AcceptPublisherNumbers,
            ));
        }
        items.push(Item::new("Continue reading", Action::Show(Screen::Book)));
        items
    }

    pub(super) fn accept_publisher_numbers(&mut self) -> Result<()> {
        let id = self
            .active
            .as_ref()
            .filter(|a| !a.reading.probe.printed_pages.is_empty())
            .ok_or("No publisher page numbers")?
            .id;
        let state = self
            .library
            .state_mut(id)
            .ok_or("Reading record unavailable")?;
        let before = (state.publisher_page_footer, state.publisher_page_margins);
        (state.publisher_page_footer, state.publisher_page_margins) = (true, true);
        self.mark_dirty();
        if let Err(error) = self.save_library() {
            let state = self.library.state_mut(id).unwrap();
            (state.publisher_page_footer, state.publisher_page_margins) = before;
            return Err(error.into());
        }
        self.show(Screen::Book);
        Ok(())
    }

    pub(super) fn restore_character_pages(&mut self) {
        let Some(active) = &self.active else { return };
        let Some(stride) = self
            .library
            .state(active.id)
            .and_then(|s| s.character_page_stride)
            .filter(|n| (500..=3000).contains(n))
        else {
            return;
        };
        if active.entry.format != DocumentFormat::Epub {
            return;
        }
        if let Some(map) = CharacterPages::cached(
            &self.numbering_path(active.id),
            active.reading.probe.book,
            active.reading.probe.chapter_count(),
            stride,
        ) {
            self.active.as_mut().unwrap().numbering = Some(map);
        } else if let Err(error) = self.start_numbering(stride) {
            self.report(error);
        }
    }

    pub(super) fn set_character_pages(&mut self, stride: Option<u16>) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a reflowable EPUB")?;
        if active.entry.format != DocumentFormat::Epub {
            return Err("Open a reflowable EPUB".into());
        }
        if let Some(stride) = stride {
            if active
                .numbering
                .as_ref()
                .is_some_and(|m| m.stride == stride)
            {
                self.numbering_scan = None;
                self.show(Screen::PublisherPages);
                return Ok(());
            }
            self.start_numbering(stride)?;
            self.show(Screen::PageNumbering);
        } else {
            self.remember_numbering(active.id, None)?;
            self.numbering_scan = None;
            let active = self.active.as_mut().unwrap();
            active.reading.probe.publisher_positions.take();
            active.numbering = None;
            self.show(Screen::Book);
        }
        Ok(())
    }

    fn start_numbering(&mut self, stride: u16) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a reflowable EPUB")?;
        let map = CharacterPages::new(active.reading.probe.book, stride)?;
        let now = Instant::now();
        self.numbering_scan = Some(Scan {
            map,
            deadline: now + SEARCH_DELAY,
            refresh: now,
        });
        Ok(())
    }

    pub(super) fn step_character_pages(&mut self, now: Instant) -> Result<Change> {
        let Some(scan) = &mut self.numbering_scan else {
            return Ok(Change::None);
        };
        let active = self.active.as_ref().ok_or("Open a reflowable EPUB")?;
        let id = active.id;
        let finished = with_storage(|| {
            // Drop the archive before returning to the event loop so USB can own the volume.
            let mut book = Book::open(&active.reading.config.book).map_err(io::Error::other)?;
            scan.map.step(&mut book).map_err(io::Error::other)
        })?;
        if !finished {
            let now = now.max(Instant::now());
            scan.deadline = now + SEARCH_DELAY;
            if self.screen == Screen::PageNumbering && now >= scan.refresh {
                scan.refresh = now + SEARCH_REFRESH;
                return Ok(Change::Partial);
            }
            return Ok(Change::None);
        }
        let map = self.numbering_scan.take().unwrap().map;
        map.save(&self.numbering_path(id))?;
        self.remember_numbering(id, Some(map.stride))?;
        let active = self.active.as_mut().unwrap();
        active.reading.probe.publisher_positions.take();
        active.numbering = Some(map);
        if self.screen == Screen::PageNumbering {
            self.show(Screen::PublisherPages);
        }
        Ok(Change::Partial)
    }

    fn remember_numbering(&mut self, id: u64, stride: Option<u16>) -> Result<()> {
        let state = self
            .library
            .state_mut(id)
            .ok_or("Reading record unavailable")?;
        let previous = state.character_page_stride;
        state.character_page_stride = stride;
        self.mark_dirty();
        if let Err(err) = self.save_library() {
            self.library.state_mut(id).unwrap().character_page_stride = previous;
            return Err(err.into());
        }
        Ok(())
    }

    pub(super) fn follow_character_page(&mut self, row: usize) -> Result<()> {
        let active = self.active.as_mut().ok_or("No open book")?;
        let map = active
            .numbering
            .as_ref()
            .ok_or("Character-count pages are unavailable")?;
        let (chapter, point) = map
            .locate(row)
            .ok_or("That stable page is outside the edition")?;
        let previous = position(&active.reading);
        if let Some(point) = point {
            let href = &active.reading.probe.chapter_entries()[chapter].href;
            let anchor = map
                .anchor(chapter, point, href)
                .ok_or("Stable page source unavailable")?;
            active.reading.jump_to_anchor(chapter, &anchor)?;
        } else {
            active.reading.jump(chapter, None)?;
        }
        self.finish_link_jump(previous);
        Ok(())
    }
}
