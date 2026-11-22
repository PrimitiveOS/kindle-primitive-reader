//! Logical font controls share the system keyboard and anchored prepare/save/swap transaction.
use super::*;

const DECREASE: Rect = Rect::new(48, LIST_Y + 286, 352, 112);
const DEFAULT: Rect = Rect::new(424, LIST_Y + 286, 416, 112);
const INCREASE: Rect = Rect::new(864, LIST_Y + 286, 352, 112);

impl Reader {
    pub(super) fn prepare_logical_font_editor(&mut self) {
        let Screen::NumberingValue(NumberField::LogicalFont(owner)) = self.screen else {
            return;
        };
        // The shared Lower layer includes the number row; Symbols does not.
        self.editor.layer = primitive_ui::Layer::Lower;
        if let Some(active) = self.active.as_ref().filter(|active| active.id == owner) {
            self.editor.text = active
                .reading
                .config
                .settings
                .logical_font_size
                .unwrap_or(crate::logical_font::DEFAULT)
                .to_string()
                .chars()
                .collect();
            self.editor.caret = self.editor.text.len();
        }
    }
    fn logical_font_default(&self) -> f64 {
        self.document_defaults
            .templates
            .reflow
            .as_ref()
            .and_then(|preferences| preferences.logical_font_size)
            .filter(|value| crate::logical_font::valid(*value))
            .unwrap_or(crate::logical_font::DEFAULT)
    }
    fn close_logical_font(&mut self) {
        self.open_reading_edge(false, WIDTH as f32 / 2.);
        self.reading_panel = Some(BarPanel::Text);
    }
    pub(super) fn logical_font_input(&mut self, input: Input) -> Option<Change> {
        let Screen::NumberingValue(NumberField::LogicalFont(_)) = self.screen else {
            return None;
        };
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            self.close_logical_font();
            return Some(Change::Partial);
        }
        let (x, y, step) = match input {
            Input::Tap(x, y) => (x, y, 0.5f64),
            Input::LongTap(x, y) => (x, y, 4.0f64),
            _ => return None,
        };
        let value = if DEFAULT.contains(x, y) {
            Ok(self.logical_font_default())
        } else if DECREASE.contains(x, y) || INCREASE.contains(x, y) {
            self.logical_font_value().map(|value| {
                if DECREASE.contains(x, y) {
                    (value - step).max(12.0)
                } else {
                    (value + step).min(255.0)
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
    fn logical_font_value(&self) -> Result<f64> {
        self.editor
            .string()
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| crate::logical_font::valid(*value))
            .ok_or_else(|| "Enter a font size from 12 to 255".into())
    }
    pub(super) fn apply_logical_font_input(&mut self, owner: u64) -> Result<()> {
        let value = self.logical_font_value()?;
        self.apply_logical_font(owner, value)
    }
    pub(super) fn apply_logical_font(&mut self, owner: u64, value: f64) -> Result<()> {
        let active = self
            .active
            .as_ref()
            .filter(|active| active.id == owner)
            .ok_or("The open book changed; reopen font size before applying")?;
        if with_storage(|| self.library.identify(&active.entry))? != owner {
            return Err(
                "The document edition changed; reopen the book before changing font size".into(),
            );
        }
        let mut config = active.reading.config.clone();
        config.settings = config.settings.with_logical_font(value)?;
        let location = position(&active.reading);
        let candidate = Reading::restore(
            config,
            location.chapter,
            location.page,
            location.anchor.as_ref(),
        )?;
        if candidate.probe.book != active.reading.probe.book
            || with_storage(|| self.library.identify(&active.entry))? != owner
        {
            return Err("The document edition changed during font sizing; reopen the book".into());
        }
        // Validate painting before committing a size: a mapped blank glyph is not success.
        let prior = self
            .library
            .state(owner)
            .cloned()
            .ok_or("Reading state unavailable")?;
        let state = self.library.state_mut(owner).unwrap();
        state.preferences.logical_font_size = Some(value);
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
        self.close_logical_font();
        Ok(())
    }
    pub(super) fn draw_logical_font(&self, canvas: &mut Canvas) {
        if !matches!(
            self.screen,
            Screen::NumberingValue(NumberField::LogicalFont(_))
        ) {
            return;
        }
        canvas.button(DECREASE, "-", false, false);
        canvas.button(
            DEFAULT,
            &format!("Default: {}", self.logical_font_default()),
            false,
            false,
        );
        canvas.button(INCREASE, "+", false, false);
        canvas.text(Rect::new(48,LIST_Y+428,1168,150),"Logical size 12 to 255. Hold -/+ for 4.\nLarge values reach the stock 340 px limit.\nContent DPI and facing pages do not change size.",CAPTION.lines(3));
    }
}

#[cfg(test)]
mod tests;
