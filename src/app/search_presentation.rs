//! Reader-wide settings for the full-text result list.
use super::list_density::{Geometry, Picker, PickerTarget};
use super::{Action, Item, Reader};
use crate::reading_controls::ReadingControls;

#[derive(Clone, Copy)]
pub(super) enum Field {
    Words,
    Rows,
}

impl Field {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Words => "Words in context",
            Self::Rows => "Results per page",
        }
    }

    pub(super) fn bounds(self) -> (u8, u8) {
        match self {
            Self::Words => (1, 50),
            Self::Rows => (2, 24),
        }
    }

    pub(super) fn default_value(self) -> u8 {
        match self {
            Self::Words => 5,
            Self::Rows => 10,
        }
    }

    fn value(self, controls: &ReadingControls) -> u8 {
        let (min, max) = self.bounds();
        let saved = match self {
            Self::Words => controls.search_context_words,
            Self::Rows => controls.search_rows,
        };
        saved
            .filter(|value| (min..=max).contains(value))
            .unwrap_or_else(|| self.default_value())
    }

    pub(super) fn set(self, controls: &mut ReadingControls, value: Option<u8>) {
        match self {
            Self::Words => controls.search_context_words = value,
            Self::Rows => controls.search_rows = value,
        }
    }
}

impl Reader {
    pub(super) fn search_presentation_items(&self) -> Vec<Item> {
        [Field::Words, Field::Rows]
            .into_iter()
            .map(|field| {
                Item::new(field.title(), Action::EditSearchPresentation(field))
                    .detail(field.value(&self.controls).to_string())
            })
            .collect()
    }

    pub(super) fn open_search_presentation(&mut self, field: Field) {
        self.notice = None;
        self.list_density_picker = Some(Picker {
            target: PickerTarget::Search(field),
            value: Some(field.value(&self.controls)),
        });
    }

    pub(super) fn match_geometry(&self) -> Geometry {
        Geometry::with_rows(Field::Rows.value(&self.controls))
    }

    pub(super) fn search_context_words(&self) -> u8 {
        Field::Words.value(&self.controls)
    }

    pub(super) fn update_search_presentation(&mut self, controls: &ReadingControls) {
        if Field::Words.value(&self.controls) != Field::Words.value(controls) {
            self.cancel_search();
            self.hits.clear();
            self.pdf_hits.clear();
            self.search_result = None;
            self.result_selection = None;
            self.pdf_result_regions.clear();
            self.focused_result = None;
            self.matches_page = 0;
        } else {
            let old_rows = self.match_geometry().rows;
            let new_rows = usize::from(Field::Rows.value(controls));
            if old_rows != new_rows {
                self.matches_page = self.matches_page * old_rows / new_rows;
                if self.screen == super::Screen::Matches {
                    self.list_page = self.list_page * old_rows / new_rows;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
