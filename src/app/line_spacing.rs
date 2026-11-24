//! KOReader-style interline percentage controls with anchored per-book updates.
use super::{step_buttons::StepButtons, *};

const DECREASE: Rect = Rect::new(48, LIST_Y + 286, 352, 112);
const DEFAULT: Rect = Rect::new(424, LIST_Y + 286, 416, 112);
const INCREASE: Rect = Rect::new(864, LIST_Y + 286, 352, 112);

impl Reader {
    pub(super) fn prepare_line_spacing_editor(&mut self) {
        let Screen::NumberingValue(NumberField::LineSpacing(owner)) = self.screen else {
            return;
        };
        self.editor.layer = primitive_ui::Layer::Lower;
        if let Some(active) = self.active.as_ref().filter(|active| active.id == owner) {
            self.editor.text = active
                .reading
                .config
                .settings
                .effective_line_spacing_percent()
                .to_string()
                .chars()
                .collect();
            self.editor.caret = self.editor.text.len();
        }
    }

    fn line_spacing_default(&self) -> u16 {
        self.document_defaults
            .templates
            .reflow
            .as_ref()
            .map(|preferences| {
                preferences
                    .line_spacing_percent
                    .filter(|&value| crate::valid_line_spacing_percent(value))
                    .unwrap_or_else(|| {
                        preferences
                            .line_height
                            .map_or(crate::DEFAULT_LINE_SPACING_PERCENT, |height| {
                                (height / 1.35 * 100.0).round().clamp(50.0, 200.0) as u16
                            })
                    })
            })
            .unwrap_or(crate::DEFAULT_LINE_SPACING_PERCENT)
    }

    fn close_line_spacing(&mut self) {
        self.open_reading_edge(false, self.navigation_layout().width as f32 * 0.5);
        self.reading_panel = Some(BarPanel::TextLayout);
    }

    pub(super) fn line_spacing_input(&mut self, input: Input) -> Option<Change> {
        let Screen::NumberingValue(NumberField::LineSpacing(_)) = self.screen else {
            return None;
        };
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if self.navigation_layout().back().contains(x, y))
        {
            self.close_line_spacing();
            return Some(Change::Partial);
        }
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps([DECREASE, DEFAULT, INCREASE], false);
        let value = if matches!(input, Input::Tap(x, y) | Input::LongTap(x, y) if default.contains(x, y))
        {
            Ok(self.line_spacing_default())
        } else if let Some(delta) = StepButtons::new(decrease, increase).delta(input, 5) {
            self.line_spacing_value()
                .map(|value| value.saturating_add_signed(delta).clamp(50, 200))
        } else {
            return None;
        };
        Some(match value {
            Ok(value) => {
                self.editor.text = value.to_string().chars().collect();
                self.editor.caret = self.editor.text.len();
                self.notice = None;
                Change::Partial
            }
            Err(error) => self.report(error),
        })
    }

    fn line_spacing_value(&self) -> Result<u16> {
        self.editor
            .string()
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|&value| crate::valid_line_spacing_percent(value))
            .ok_or_else(|| "Enter line spacing from 50% to 200%".into())
    }

    pub(super) fn apply_line_spacing_input(&mut self, owner: u64) -> Result<()> {
        self.apply_line_spacing(owner, self.line_spacing_value()?)
    }

    pub(super) fn apply_line_spacing(&mut self, owner: u64, value: u16) -> Result<()> {
        if !crate::valid_line_spacing_percent(value) {
            return Err("Enter line spacing from 50% to 200%".into());
        }
        let active = self
            .active
            .as_ref()
            .filter(|active| active.id == owner)
            .ok_or("The open book changed; reopen line spacing before applying")?;
        if with_storage(|| self.library.identify(&active.entry))? != owner {
            return Err(
                "The document edition changed; reopen it before changing line spacing".into(),
            );
        }
        let location = position(&active.reading);
        let mut config = active.reading.config.clone();
        config.settings = config.settings.with_line_spacing_percent(value);
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
            return Err("The document edition changed during line spacing; reopen the book".into());
        }
        let prior = self
            .library
            .state(owner)
            .cloned()
            .ok_or("Reading state unavailable")?;
        let state = self.library.state_mut(owner).unwrap();
        state.preferences.line_spacing_percent = Some(value);
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
        self.close_line_spacing();
        Ok(())
    }

    pub(super) fn draw_line_spacing(&self, canvas: &mut Canvas) {
        if !matches!(
            self.screen,
            Screen::NumberingValue(NumberField::LineSpacing(_))
        ) {
            return;
        }
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps([DECREASE, DEFAULT, INCREASE], false);
        StepButtons::new(decrease, increase).draw(canvas);
        canvas.button(
            default,
            &format!("Default: {}%", self.line_spacing_default()),
            false,
            false,
        );
        canvas.text(
            layout.detail(Rect::new(48, LIST_Y + 428, 1168, 150), false),
            "Line spacing from 50% to 200%.\nTap −/+ for 1%; hold for 5%.\nThe default is 100% unless changed in Document defaults.",
            CAPTION.lines(3),
        );
    }
}

#[cfg(test)]
mod tests;
