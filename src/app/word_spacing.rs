//! Two percentage fields share one numeric keyboard and one atomic book update.
use super::*;
use crate::WordSpacePercent;

const SCALING: Rect = Rect::new(48, LIST_Y + 150, 572, 112);
const MINIMUM: Rect = Rect::new(644, LIST_Y + 150, 572, 112);
const DECREASE: Rect = Rect::new(48, LIST_Y + 286, 352, 112);
const DEFAULT: Rect = Rect::new(424, LIST_Y + 286, 416, 112);
const INCREASE: Rect = Rect::new(864, LIST_Y + 286, 352, 112);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Field {
    Scaling,
    Minimum,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Owner {
    Book(u64),
    Profile(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Draft {
    owner: Owner,
    pub(super) value: WordSpacePercent,
    pub(super) field: Field,
}

impl Draft {
    pub(super) fn profile(index: usize, value: Option<WordSpacePercent>) -> Self {
        Self {
            owner: Owner::Profile(index),
            value: value.unwrap_or(WordSpacePercent::MEDIUM),
            field: Field::Scaling,
        }
    }

    pub(super) fn profile_index(self) -> Option<usize> {
        match self.owner {
            Owner::Profile(index) => Some(index),
            Owner::Book(_) => None,
        }
    }
}

impl Reader {
    pub(super) fn word_spacing_draft(&self, owner: u64) -> Draft {
        let value = self
            .active
            .as_ref()
            .filter(|active| active.id == owner)
            .and_then(|active| active.reading.config.settings.percent_word_spacing)
            .unwrap_or(WordSpacePercent::MEDIUM);
        Draft {
            owner: Owner::Book(owner),
            value,
            field: Field::Scaling,
        }
    }

    fn word_spacing_default(&self, owner: Owner) -> WordSpacePercent {
        if matches!(owner, Owner::Profile(_)) {
            return WordSpacePercent::MEDIUM;
        }
        self.document_defaults
            .templates
            .reflow
            .as_ref()
            .and_then(|p| p.percent_word_spacing)
            .filter(|pair| pair.valid())
            .unwrap_or(WordSpacePercent::MEDIUM)
    }

    pub(super) fn prepare_word_spacing_editor(&mut self) {
        let Screen::NumberingValue(NumberField::WordSpacing(draft)) = self.screen else {
            return;
        };
        self.editor.layer = primitive_ui::Layer::Lower;
        let number = match draft.field {
            Field::Scaling => draft.value.scaling,
            Field::Minimum => u16::from(draft.value.minimum),
        };
        self.editor.text = number.to_string().chars().collect();
        self.editor.caret = self.editor.text.len();
    }

    fn parsed_word_spacing_draft(&self, mut draft: Draft) -> Result<Draft> {
        let number = self.editor.string().trim().parse::<u16>().ok();
        match draft.field {
            Field::Scaling => {
                draft.value.scaling = number
                    .filter(|n| (10..=500).contains(n))
                    .ok_or("Scaling must be a whole percentage from 10 to 500")?
            }
            Field::Minimum => {
                draft.value.minimum = number
                    .filter(|n| (25..=100).contains(n))
                    .ok_or("Reduction must be a whole percentage from 25 to 100")?
                    as u8
            }
        }
        if !draft.value.valid() {
            return Err("The spacing percentages are out of range".into());
        }
        Ok(draft)
    }

    fn close_word_spacing(&mut self) {
        self.open_reading_edge(false, self.navigation_layout().width as f32 * 0.5);
        self.reading_panel = Some(BarPanel::TextSpacing);
    }

    pub(super) fn word_spacing_input(&mut self, input: Input) -> Option<Change> {
        let Screen::NumberingValue(NumberField::WordSpacing(draft)) = self.screen else {
            return None;
        };
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if self.navigation_layout().back().contains(x,y))
        {
            if let Some(index) = draft.profile_index() {
                self.show(Screen::ReadingProfileValues(index));
            } else {
                self.close_word_spacing();
            }
            return Some(Change::Partial);
        }
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps([DECREASE, DEFAULT, INCREASE], true);
        let [scaling, minimum] = layout.choices([SCALING, MINIMUM]);
        let (x, y, step) = match input {
            Input::Tap(x, y) => (x, y, 1u16),
            Input::LongTap(x, y) => (x, y, 10u16),
            _ => return None,
        };
        let result = if default.contains(x, y) {
            Ok(Draft {
                value: self.word_spacing_default(draft.owner),
                ..draft
            })
        } else if scaling.contains(x, y) || minimum.contains(x, y) {
            self.parsed_word_spacing_draft(draft).map(|mut next| {
                next.field = if scaling.contains(x, y) {
                    Field::Scaling
                } else {
                    Field::Minimum
                };
                next
            })
        } else if decrease.contains(x, y) || increase.contains(x, y) {
            self.parsed_word_spacing_draft(draft).map(|mut next| {
                let (old, lower, upper) = match next.field {
                    Field::Scaling => (next.value.scaling, 10, 500),
                    Field::Minimum => (u16::from(next.value.minimum), 25, 100),
                };
                let value = if decrease.contains(x, y) {
                    old.saturating_sub(step).max(lower)
                } else {
                    old.saturating_add(step).min(upper)
                };
                match next.field {
                    Field::Scaling => next.value.scaling = value,
                    Field::Minimum => next.value.minimum = value as u8,
                }
                next
            })
        } else {
            return None;
        };
        Some(match result {
            Ok(next) => {
                self.screen = Screen::NumberingValue(NumberField::WordSpacing(next));
                self.prepare_word_spacing_editor();
                self.notice = None;
                Change::Partial
            }
            Err(error) => self.report(error),
        })
    }

    pub(super) fn apply_word_spacing_input(&mut self, draft: Draft) -> Result<()> {
        let value = self.parsed_word_spacing_draft(draft)?.value;
        match draft.owner {
            Owner::Book(id) => self.apply_word_spacing(id, value),
            Owner::Profile(index) => self.save_profile_word_spacing(index, value),
        }
    }

    pub(super) fn apply_word_spacing(&mut self, owner: u64, value: WordSpacePercent) -> Result<()> {
        if !value.valid() {
            return Err("The spacing percentages are out of range".into());
        }
        let active = self
            .active
            .as_ref()
            .filter(|active| active.id == owner)
            .ok_or("The open book changed; reopen word spacing before applying")?;
        if with_storage(|| self.library.identify(&active.entry))? != owner {
            return Err(
                "The document edition changed; reopen it before changing word spacing".into(),
            );
        }
        let location = position(&active.reading);
        let mut config = active.reading.config.clone();
        config.settings.percent_word_spacing = Some(value);
        config.prepare_page_layout(&active.reading.config);
        let candidate = Reading::restore(
            config,
            location.chapter,
            location.page,
            location.anchor.as_ref(),
        )?;
        if candidate.probe.book != active.reading.probe.book
            || with_storage(|| self.library.identify(&active.entry))? != owner
        {
            return Err("The document edition changed during word spacing; reopen it".into());
        }
        let prior = self
            .library
            .state(owner)
            .cloned()
            .ok_or("Reading state unavailable")?;
        let state = self.library.state_mut(owner).unwrap();
        state.preferences.percent_word_spacing = Some(value);
        state.position = Some(position(&candidate));
        let prior_dirty = self.dirty;
        let prior_save_at = self.save_at;
        self.dirty = true;
        if let Err(error) = self.save_library() {
            *self.library.state_mut(owner).unwrap() = prior;
            self.dirty = prior_dirty;
            self.save_at = prior_save_at;
            return Err(error.into());
        }
        self.active.as_mut().unwrap().reading = candidate;
        self.record_position();
        self.close_word_spacing();
        Ok(())
    }

    pub(super) fn draw_word_spacing(&self, canvas: &mut Canvas) {
        let Screen::NumberingValue(NumberField::WordSpacing(draft)) = self.screen else {
            return;
        };
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps([DECREASE, DEFAULT, INCREASE], true);
        let [scaling, minimum] = layout.choices([SCALING, MINIMUM]);
        let value = self
            .parsed_word_spacing_draft(draft)
            .map_or(draft.value, |d| d.value);
        canvas.button(
            scaling,
            &format!("Scaling: {}%", value.scaling),
            draft.field == Field::Scaling,
            false,
        );
        canvas.button(
            minimum,
            &format!("Reduction: {}%", value.minimum),
            draft.field == Field::Minimum,
            false,
        );
        canvas.button(decrease, "-", false, false);
        let saved_default = self.word_spacing_default(draft.owner);
        canvas.button(
            default,
            &format!(
                "Default: {}/{}%",
                saved_default.scaling, saved_default.minimum
            ),
            false,
            false,
        );
        canvas.button(increase, "+", false, false);
        canvas.text(layout.detail(Rect::new(48, LIST_Y + 428, 1168, 200), true),
            "Scaling changes natural space width.\nReduction is the minimum remaining width.\n100% prevents reduction; hold -/+ for 10%.",CAPTION.lines(3));
    }
}

#[cfg(test)]
mod tests;
