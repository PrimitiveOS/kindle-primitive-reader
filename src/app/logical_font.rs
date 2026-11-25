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
            self.editor.text = self
                .pending_logical_size(owner)
                .or(active.reading.config.settings.logical_font_size)
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
        self.open_reading_edge(false, self.navigation_layout().width as f32 * 0.5);
        self.reading_panel = Some(BarPanel::Text);
    }
    pub(super) fn logical_font_input(&mut self, input: Input) -> Option<Change> {
        let Screen::NumberingValue(NumberField::LogicalFont(_)) = self.screen else {
            return None;
        };
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if self.navigation_layout().back().contains(x,y))
        {
            self.close_logical_font();
            return Some(Change::Partial);
        }
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps([DECREASE, DEFAULT, INCREASE], false);
        let (x, y, step) = match input {
            Input::Tap(x, y) => (x, y, 0.5f64),
            Input::LongTap(x, y) => (x, y, 4.0f64),
            _ => return None,
        };
        let value = if default.contains(x, y) {
            Ok(self.logical_font_default())
        } else if decrease.contains(x, y) || increase.contains(x, y) {
            self.logical_font_value().map(|value| {
                if decrease.contains(x, y) {
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
        let mut config = active.reading.config.clone();
        config.settings = config.settings.with_logical_font(value)?;
        config.prepare_page_layout(&active.reading.config);
        let location = position(&active.reading);
        let publication = active.reading.probe.book;
        let entry = active.entry.clone();
        if self.queue_eager_font(owner, value, &config) {
            return Ok(());
        }
        if with_storage(|| self.library.identify(&entry))? != owner {
            return Err(
                "The document edition changed; reopen the book before changing font size".into(),
            );
        }
        let candidate = Reading::restore(
            config,
            location.chapter,
            location.page,
            location.anchor.as_ref(),
        )?;
        if candidate.probe.book != publication
            || with_storage(|| self.library.identify(&entry))? != owner
        {
            return Err("The document edition changed during font sizing; reopen the book".into());
        }
        self.commit_logical_font(owner, value, candidate)?;
        self.close_logical_font();
        Ok(())
    }

    pub(super) fn commit_logical_font(
        &mut self,
        owner: u64,
        value: f64,
        candidate: Reading,
    ) -> Result<()> {
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
        Ok(())
    }
    pub(super) fn draw_logical_font(&self, canvas: &mut Canvas) {
        if !matches!(
            self.screen,
            Screen::NumberingValue(NumberField::LogicalFont(_))
        ) {
            return;
        }
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps([DECREASE, DEFAULT, INCREASE], false);
        canvas.button(decrease, "-", false, false);
        canvas.button(
            default,
            &format!("Default: {:.1}", self.logical_font_default()),
            false,
            false,
        );
        canvas.button(increase, "+", false, false);
        canvas.text(layout.detail(Rect::new(48, LIST_Y + 428, 1168, 150), false),"Logical size 12 to 255. Hold -/+ for 4.\nLarge values reach the stock 340 px limit.\nContent DPI and facing pages do not change size.",CAPTION.lines(3));
    }
}

#[cfg(test)]
mod tests;

/// A viewport failure is recoverable without changing the document or silently
/// overwriting the imported choice. Reset is an explicit modal transaction.
#[derive(Clone)]
pub(super) struct Recovery {
    pub(super) entry: LibraryBook,
    pub(super) id: u64,
    pub(super) config: BookConfig,
    pub(super) position: Option<ReadingPosition>,
    pub(super) preferences: BookPreferences,
}
impl Reader {
    fn logical_recovery_sheet() -> primitive_ui::Sheet<'static> {
        primitive_ui::Sheet::centered(
            Rect::new(0, 0, WIDTH as i32, HEIGHT as i32),
            "Text size does not fit",
            "Reset text to fit? Old settings are backed up; notes and position stay.",
            "Reset text",
        )
    }
    pub(super) fn draw_logical_recovery(&self, canvas: &mut Canvas) {
        if self.logical_recovery.is_some() {
            Self::logical_recovery_sheet().draw(canvas, None);
        }
    }
    pub(super) fn logical_recovery_input(&mut self, input: Input) -> Option<Change> {
        self.logical_recovery.as_ref()?;
        let reset = match input {
            Input::Sleep | Input::Wake => return None,
            Input::PageBack => false,
            Input::Tap(x, y) => match Self::logical_recovery_sheet().hit(x, y) {
                Some(primitive_ui::SheetAction::Cancel) => false,
                Some(primitive_ui::SheetAction::Confirm) => true,
                None => return Some(Change::None),
            },
            _ => return Some(Change::None),
        };
        if !reset {
            self.logical_recovery = None;
            return Some(Change::Partial);
        }
        Some(match self.reset_unopened_logical_layout() {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }
    fn reset_unopened_logical_layout(&mut self) -> Result<()> {
        let recovery = self
            .logical_recovery
            .clone()
            .ok_or("No saved layout to recover")?;
        if with_storage(|| self.library.identify(&recovery.entry))? != recovery.id {
            return Err("The document edition changed; reopen it before resetting text".into());
        }
        let preferences = BookPreferences::for_new_reflow();
        let mut config = recovery.config.clone();
        config.settings = preferences.apply_to(self.device_reading_settings_for(config.viewport));
        let top_status_header = self.top_header_for_book(recovery.id);
        config.settings.status_header_height =
            crate::reading_controls::TopHeader::height(top_status_header, false);
        config.fonts = self.fonts.clone();
        config.scroll = false;
        config.facing_pages = false;
        config.prepare_page_layout(&recovery.config);
        let candidate = if let Some(saved) = &recovery.position {
            Reading::restore(config, saved.chapter, saved.page, saved.anchor.as_ref())?
        } else {
            Reading::open(config)?
        };
        if with_storage(|| self.library.identify(&recovery.entry))? != recovery.id {
            return Err("The document edition changed while resetting text".into());
        }
        crate::document_defaults::validate_preferences(&recovery.preferences)?;
        let bytes = serde_json::to_vec(
            &serde_json::json!({"version":1,"book":recovery.id,"preferences":recovery.preferences}),
        )?;
        let fingerprint = crate::library::fingerprint_bytes(&bytes, b"layout-recovery");
        let folder = self
            .profiles_path
            .parent()
            .ok_or("Reader data folder unavailable")?
            .join("layout-recovery");
        let path = folder.join(format!("{:016x}-{fingerprint:016x}.json", recovery.id));
        with_storage(|| {
            fs::create_dir_all(&folder)?;
            if path.exists() {
                if fs::read(&path)? != bytes {
                    return Err(io::Error::other(
                        "A different layout backup already uses this filename",
                    ));
                }
                Ok(())
            } else {
                primitive_app::save(&path, &bytes)
            }
        })?;
        self.library.commit_recovered_layout(
            &recovery.entry,
            recovery.id,
            preferences,
            position(&candidate),
            top_status_header,
        )?;
        let closing_warning = self.finish_before_switch(recovery.id);
        self.fixed = None;
        self.footnote = None;
        self.selection = None;
        let header_metadata =
            top_status_header.then(|| self.load_header_metadata(recovery.entry.library_path()));
        self.active = Some(ActiveBook {
            entry: recovery.entry,
            id: recovery.id,
            reading: candidate,
            numbering: None,
            embedded_font_inventory: None,
            header_metadata,
        });
        self.logical_recovery = None;
        self.pages_since_refresh = 0;
        self.range_edit = None;
        self.saved_popup = None;
        self.style_view = Default::default();
        self.restore_character_pages();
        self.clear_location_history();
        self.auto_turn.configure(None, Instant::now());
        self.annotation_search = annotation_search::AnnotationSearch::default();
        self.restore_reading_menus(recovery.id);
        self.reading_history.begin_session(crate::mark_time::now());
        self.show(Screen::Book);
        self.reset_footer_initial_position();
        self.remember_opening_location();
        self.record_position();
        self.notice = Some(
            closing_warning
                .unwrap_or_else(|| "Text settings reset. Previous choices backed up.".into()),
        );
        Ok(())
    }
}
