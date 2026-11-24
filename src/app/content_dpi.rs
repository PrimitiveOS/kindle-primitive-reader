//! The shared numeric editor for content DPI; screen and reader-font DPI stay separate.
use super::*;

const DECREASE: Rect = Rect::new(48, LIST_Y + 286, 352, 112);
const DEFAULT: Rect = Rect::new(424, LIST_Y + 286, 416, 112);
const INCREASE: Rect = Rect::new(864, LIST_Y + 286, 352, 112);

impl Reader {
    pub(super) fn prepare_content_dpi_editor(&mut self) {
        let Screen::NumberingValue(NumberField::ContentDpi(owner)) = self.screen else {
            return;
        };
        // The shared Lower layer includes the number row; Symbols does not.
        self.editor.layer = primitive_ui::Layer::Lower;
        if let Some(active) = self.active.as_ref().filter(|active| active.id == owner) {
            self.editor.text = active
                .reading
                .config
                .settings
                .render_dpi
                .to_string()
                .chars()
                .collect();
            self.editor.caret = self.editor.text.len();
        }
    }
    fn content_dpi_default(&self) -> u16 {
        self.document_defaults
            .templates
            .reflow
            .as_ref()
            .and_then(|preferences| preferences.render_dpi)
            .filter(|dpi| crate::valid_render_dpi(*dpi))
            .unwrap_or(Settings::default().render_dpi)
    }
    fn close_content_dpi(&mut self) {
        self.open_reading_edge(false, self.navigation_layout().width as f32 * 0.5);
        self.reading_panel = Some(BarPanel::TextRendering);
    }
    pub(super) fn content_dpi_input(&mut self, input: Input) -> Option<Change> {
        let Screen::NumberingValue(NumberField::ContentDpi(_)) = self.screen else {
            return None;
        };
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if self.navigation_layout().back().contains(x,y))
        {
            self.close_content_dpi();
            return Some(Change::Partial);
        }
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps([DECREASE, DEFAULT, INCREASE], false);
        let (x, y, step) = match input {
            Input::Tap(x, y) => (x, y, 1u16),
            Input::LongTap(x, y) => (x, y, 20u16),
            _ => return None,
        };
        let value = if default.contains(x, y) {
            Ok(self.content_dpi_default())
        } else if decrease.contains(x, y) || increase.contains(x, y) {
            self.content_dpi_value().map(|value| {
                if decrease.contains(x, y) {
                    value.saturating_sub(step)
                } else {
                    value.saturating_add(step).min(300)
                }
            })
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
    fn content_dpi_value(&self) -> Result<u16> {
        self.editor
            .string()
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|value| crate::valid_render_dpi(*value))
            .ok_or_else(|| "Enter a whole number from 0 to 300 dpi".into())
    }
    pub(super) fn apply_content_dpi(&mut self, owner: u64) -> Result<()> {
        let active = self
            .active
            .as_ref()
            .filter(|active| active.id == owner)
            .ok_or("The open book changed; reopen content zoom before applying")?;
        let value = self.content_dpi_value()?;
        if with_storage(|| self.library.identify(&active.entry))? != owner {
            return Err(
                "The document edition changed; reopen the book before changing content zoom".into(),
            );
        }
        let mut config = active.reading.config.clone();
        config.settings.render_dpi = value;
        let location = position(&active.reading);
        // Prepare without touching the current page, selection or saved model.
        let candidate = Reading::restore(
            config,
            location.chapter,
            location.page,
            location.anchor.as_ref(),
        )?;
        if candidate.probe.book != active.reading.probe.book
            || with_storage(|| self.library.identify(&active.entry))? != owner
        {
            return Err("The document edition changed during content zoom; reopen the book".into());
        }
        let prior = self
            .library
            .state(owner)
            .cloned()
            .ok_or("Reading state unavailable")?;
        let state = self.library.state_mut(owner).unwrap();
        state.preferences.render_dpi = Some(value);
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
        self.close_content_dpi();
        Ok(())
    }
    pub(super) fn draw_content_dpi(&self, canvas: &mut Canvas) {
        if !matches!(
            self.screen,
            Screen::NumberingValue(NumberField::ContentDpi(_))
        ) {
            return;
        }
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps([DECREASE, DEFAULT, INCREASE], false);
        canvas.button(decrease, "-", false, false);
        canvas.button(
            default,
            &format!("Default: {} dpi", self.content_dpi_default()),
            false,
            false,
        );
        canvas.button(increase, "+", false, false);
        canvas.text(layout.detail(Rect::new(48, LIST_Y + 428, 1168, 150), false),"0: Off. 96: original pixel scale.\nHold -/+ to change by 20 dpi.\nText size and screen DPI stay unchanged.",CAPTION.lines(3));
    }
}

#[cfg(test)]
mod tests;
