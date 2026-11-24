//! A preset picker borrows the current query without changing source preferences.
use super::*;

pub(super) struct PresetPicker {
    query: String,
    page: usize,
    replace_miss: bool,
}

const PRESETS_PER_PAGE: usize = 5;
const SEARCH_WITH_PRESET: &str = "Search with preset";

pub(super) struct PresetLayout {
    panel: Rect,
    shown: usize,
    paging: bool,
}

impl PresetLayout {
    pub(super) fn close(&self) -> Rect {
        Rect::new(self.panel.right() - 220, self.panel.y + 24, 188, 100)
    }

    pub(super) fn row(&self, slot: usize) -> Rect {
        Rect::new(
            self.panel.x + 32,
            self.panel.y + 146 + slot as i32 * 112,
            self.panel.w - 64,
            100,
        )
    }

    fn pages(&self) -> (Rect, Rect) {
        let y = self.panel.bottom() - 122;
        (
            Rect::new(self.panel.x + 32, y, 220, 100),
            Rect::new(self.panel.right() - 252, y, 220, 100),
        )
    }
}

impl Reader {
    pub(in crate::app) fn dictionary_preset_picker_open(&self) -> bool {
        self.dictionary_navigation.preset_picker.is_some()
    }

    pub(in crate::app) fn close_dictionary_preset_picker(&mut self) {
        self.dictionary_navigation.preset_picker = None;
    }

    pub(in crate::app) fn lookup_preset_button(&self) -> Option<Rect> {
        if self.dictionary_settings.presets.is_empty() || self.dictionary_preset_picker_open() {
            return None;
        }
        match self.screen {
            Screen::DictionaryEntry => Some(Rect::new(40, 390, 395, 100)),
            Screen::Definition => match self.dictionary_navigation.view.query_editing {
                Some(QueryEdit::Word) => Some(Rect::new(40, 410, 395, 100)),
                Some(QueryEdit::FuzzyRetry) => {
                    let panel = self.fuzzy_retry_layout().0;
                    Some(Rect::new(panel.right() - 425, panel.y + 219, 395, 100))
                }
                None if self.can_retry_with_preset() => {
                    let rect = body(self.definition_layout().0);
                    Some(Rect::new(rect.x, rect.y + 70, 395, 100))
                }
                None => None,
            },
            _ => None,
        }
    }

    pub(super) fn can_retry_with_preset(&self) -> bool {
        !self.definition_word.is_empty()
            && self.definitions.is_empty()
            && self.spelling_lookup.is_none()
            && !self.dictionary_settings.presets.is_empty()
    }

    pub(in crate::app) fn draw_lookup_preset_button(&self, canvas: &mut Canvas) {
        if let Some(button) = self.lookup_preset_button() {
            canvas.button(button, SEARCH_WITH_PRESET, false, false);
        }
    }

    pub(in crate::app) fn open_dictionary_preset_picker(
        &mut self,
        x: f32,
        y: f32,
    ) -> Option<Change> {
        if !self.lookup_preset_button()?.contains(x, y) {
            return None;
        }
        let (query, replace_miss) = if self.screen == Screen::Definition {
            let edit = self.dictionary_navigation.view.query_editing;
            let query = if edit.is_some() {
                self.editor.string()
            } else {
                self.definition_word.clone()
            };
            (query, self.definitions.is_empty())
        } else {
            (self.editor.string(), false)
        };
        if !lookup_history::valid(&query) {
            return Some(self.report("Enter a word or a short phrase"));
        }
        self.notice = None;
        self.dictionary_navigation.preset_picker = Some(PresetPicker {
            query,
            page: 0,
            replace_miss,
        });
        Some(Change::Partial)
    }

    pub(super) fn preset_picker_layout(&self, page: usize) -> PresetLayout {
        let count = self.dictionary_settings.presets.names().len();
        let shown = count
            .saturating_sub(page * PRESETS_PER_PAGE)
            .min(PRESETS_PER_PAGE);
        let paging = count > PRESETS_PER_PAGE;
        let width = 1024;
        let height = 158 + shown as i32 * 112 + if paging { 126 } else { 0 };
        let navigation = self.navigation_layout();
        PresetLayout {
            panel: Rect::new(
                (navigation.width - width) / 2,
                (navigation.height - height) / 2,
                width,
                height,
            ),
            shown,
            paging,
        }
    }

    pub(in crate::app) fn draw_dictionary_preset_picker(&self, canvas: &mut Canvas) {
        let Some(picker) = &self.dictionary_navigation.preset_picker else {
            return;
        };
        let face = self.preset_picker_layout(picker.page);
        let panel = face.panel;
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 32, panel.y + 32, panel.w - 282, 88),
            SEARCH_WITH_PRESET,
            BODY.bold(),
        );
        canvas.button(face.close(), "Cancel", false, false);
        for (slot, name) in self
            .dictionary_settings
            .presets
            .names()
            .skip(picker.page * PRESETS_PER_PAGE)
            .take(face.shown)
            .enumerate()
        {
            let row = face.row(slot);
            canvas.button(row, "", false, false);
            canvas.text(
                Rect::new(row.x + 26, row.y + 23, row.w - 52, row.h - 28),
                name,
                BODY.lines(2).ink(if canvas.is_control_pressed(row) {
                    255
                } else {
                    0
                }),
            );
        }
        if face.paging {
            let (previous, next) = face.pages();
            if picker.page > 0 {
                canvas.button(previous, "Previous", false, false);
            }
            if (picker.page + 1) * PRESETS_PER_PAGE < self.dictionary_settings.presets.names().len()
            {
                canvas.button(next, "Next", false, false);
            }
        }
    }

    pub(in crate::app) fn dictionary_preset_picker_input(
        &mut self,
        input: Input,
    ) -> Option<Change> {
        let picker = self.dictionary_navigation.preset_picker.as_ref()?;
        let face = self.preset_picker_layout(picker.page);
        let (previous, next) = face.pages();
        let step = match input {
            Input::Tap(x, y) if !face.panel.contains(x, y) || face.close().contains(x, y) => {
                self.close_dictionary_preset_picker();
                return Some(Change::Partial);
            }
            Input::PageBack if picker.page == 0 => {
                self.close_dictionary_preset_picker();
                return Some(Change::Partial);
            }
            Input::PageBack => -1,
            Input::PageForward => 1,
            Input::Swipe { dx, dy, .. } if dx.abs().max(dy.abs()) > 0.0 => {
                if (if dx.abs() > dy.abs() { dx } else { dy }) < 0.0 {
                    1
                } else {
                    -1
                }
            }
            Input::Tap(x, y) if face.paging && previous.contains(x, y) => -1,
            Input::Tap(x, y) if face.paging && next.contains(x, y) => 1,
            Input::Tap(x, y) => {
                let Some(slot) = (0..face.shown).find(|&slot| face.row(slot).contains(x, y)) else {
                    return Some(Change::None);
                };
                let index = picker.page * PRESETS_PER_PAGE + slot;
                let picker = self.dictionary_navigation.preset_picker.take()?;
                let result = self
                    .dictionary_settings
                    .presets
                    .get(index)
                    .map(|(_, members)| members.to_vec());
                return Some(match result {
                    Ok(members) => {
                        let mode = LookupMode::Preset {
                            members,
                            replace_miss: picker.replace_miss,
                        };
                        match self.look_up_entry(&picker.query, mode) {
                            Ok(()) => Change::Partial,
                            Err(error) => self.report(error),
                        }
                    }
                    Err(error) => self.report(error),
                });
            }
            _ => return Some(Change::None),
        };
        let Some(page) = picker.page.checked_add_signed(step).filter(|page| {
            page * PRESETS_PER_PAGE < self.dictionary_settings.presets.names().len()
        }) else {
            return Some(Change::None);
        };
        self.dictionary_navigation.preset_picker.as_mut()?.page = page;
        Some(Change::Partial)
    }
}
