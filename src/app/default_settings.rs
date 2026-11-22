//! Save format defaults separately from source-specific progress and geometry.
use super::*;
use crate::document_defaults::{self, FixedDefaults};

#[derive(Clone, Copy)]
pub(super) enum Command {
    SaveReflow,
    ResetReflow,
    ClearReflow,
    SaveFixed,
    ResetFixed,
    ClearFixed,
}

impl Reader {
    pub(super) fn current_text_preferences(&self) -> Result<BookPreferences> {
        let active = self.active.as_ref().ok_or("Open a book first")?;
        let config = &active.reading.config;
        let settings = config.settings;
        let font_file = self
            .library
            .state(active.id)
            .and_then(|state| state.preferences.font_file.clone());
        Ok(BookPreferences {
            font_size: if settings.logical_font_size.is_some() {
                self.library
                    .state(active.id)
                    .and_then(|state| state.preferences.font_size)
            } else {
                Some(settings.font_size)
            },
            logical_font_size: settings.logical_font_size,
            render_dpi: Some(settings.render_dpi),
            margin: Some(settings.margin),
            margin_sides: settings.margin_sides,
            margin_left: settings.margin_left,
            margin_right: settings.margin_right,
            link_vertical_margins: settings.link_vertical_margins,
            margin_top: settings.margin_top,
            margin_bottom: settings.margin_bottom,
            line_height: Some(settings.line_height),
            word_spacing: settings.word_spacing,
            percent_word_spacing: settings.percent_word_spacing,
            word_expansion: Some(settings.word_expansion),
            letter_spacing: settings.letter_spacing,
            cjk_width_percent: (settings.cjk_width_percent != 100)
                .then_some(settings.cjk_width_percent),
            scroll: config.scroll,
            facing_pages: config.facing_pages,
            alignment: settings.alignment,
            bold_text: settings.bold_text,
            text_contrast: settings.text_contrast,
            font_gamma_index: Some(settings.font_gamma_index),
            font_weight_delta: Some(settings.font_weight_delta),
            publisher_styles: Some(settings.publisher_styles),
            font_kerning: Some(settings.font_kerning),
            font_hinting: settings.font_hinting,
            smooth_images: Some(settings.smooth_images),
            invert_night_images: settings.invert_night_images,
            override_fonts: settings.override_fonts,
            disable_soft_hyphens: settings.disable_soft_hyphens,
            automatic_hyphenation: Some(settings.automatic_hyphenation),
            font_file,
        })
    }
}

impl Reader {
    fn default_reflow_preferences(&self) -> (BookPreferences, bool) {
        let mut prefs = self
            .document_defaults
            .templates
            .reflow
            .clone()
            .unwrap_or_else(BookPreferences::for_new_reflow);
        let missing =
            prefs.font_file.is_some() && self.fonts_for(prefs.font_file.as_deref()).is_err();
        if missing {
            prefs.font_file = None;
        }
        (prefs, missing)
    }
    pub(super) fn activate_document_defaults(&mut self, command: Command) -> Result<()> {
        let mut templates = self.document_defaults.templates.clone();
        let message = match command {
            Command::SaveReflow => {
                let prefs = self.current_text_preferences()?;
                document_defaults::validate_preferences(&prefs)?;
                templates.reflow = Some(prefs);
                self.document_defaults.save(templates)?;
                "Text settings saved for new books"
            }
            Command::SaveFixed => {
                let fixed = self
                    .fixed
                    .as_ref()
                    .ok_or("Open a fixed-page document first")?;
                templates.fixed = Some(FixedDefaults::from_view(fixed.reading.preferences()));
                self.document_defaults.save(templates)?;
                "Page settings saved for new documents; crop and view offset excluded"
            }
            Command::ClearReflow => {
                templates.reflow = None;
                self.document_defaults.save(templates)?;
                "New reflowable books will use device defaults"
            }
            Command::ClearFixed => {
                templates.fixed = None;
                self.document_defaults.save(templates)?;
                "New fixed-page documents will use device defaults"
            }
            Command::ResetReflow => return self.reset_reflow_defaults(),
            Command::ResetFixed => return self.reset_fixed_defaults(),
        };
        self.show(Screen::DocumentDefaults);
        self.notice = Some(message.into());
        Ok(())
    }

    fn reset_reflow_defaults(&mut self) -> Result<()> {
        let (prefs, missing_font) = self.default_reflow_preferences();
        let fonts = self.fonts_for(prefs.font_file.as_deref())?;
        let base = self.device_reading_settings();
        let active = self.active.as_ref().ok_or("Open a reflowable book first")?;
        let location = position(&active.reading);
        let mut config = active.reading.config.clone();
        config.settings = prefs.apply_to(base);
        config.scroll = prefs.scroll;
        config.facing_pages = prefs.facing_pages;
        config.fonts = fonts;
        // Keep the old reading object/frame until both preparation and the
        // atomic library write succeed. A failed reflow cannot alter the view.
        let candidate = Reading::restore(
            config,
            location.chapter,
            location.page,
            location.anchor.as_ref(),
        )?;
        let id = active.id;
        let old = self
            .library
            .state(id)
            .cloned()
            .ok_or("Reading state unavailable")?;
        self.library.state_mut(id).unwrap().preferences = prefs;
        if let Err(error) = self.library.persist() {
            *self.library.state_mut(id).unwrap() = old;
            return Err(error.into());
        }
        self.active.as_mut().unwrap().reading = candidate;
        self.record_position();
        self.show(Screen::DocumentDefaults);
        self.notice = Some(
            if missing_font {
                "Book reset; default font unavailable, using device fonts"
            } else {
                "Book reset to text defaults; reading position and annotations kept"
            }
            .into(),
        );
        Ok(())
    }

    fn reset_fixed_defaults(&mut self) -> Result<()> {
        let template = self.document_defaults.templates.fixed.unwrap_or_default();
        let fixed = self
            .fixed
            .as_mut()
            .ok_or("Open a fixed-page document first")?;
        let view = template.apply(fixed.reading.preferences());
        let id = fixed.id;
        let library = &mut self.library;
        let old = library
            .state(id)
            .cloned()
            .ok_or("Reading state unavailable")?;
        let result = fixed.reading.restore_view_with(view, |page, effective| {
            let state = library
                .state_mut(id)
                .ok_or_else(|| io::Error::other("Reading state unavailable"))?;
            state.fixed_preferences = effective;
            state.position = Some(ReadingPosition {
                chapter: 0,
                page,
                anchor: None,
            });
            library.persist()
        });
        if let Err(error) = result {
            *library.state_mut(id).unwrap() = old;
            return Err(error);
        }
        self.record_position();
        self.show(Screen::DocumentDefaults);
        self.notice =
            Some("Document reset to page defaults; crop and annotations kept; pan kept within page bounds".into());
        Ok(())
    }
}

fn controls(fixed: bool) -> [(Rect, Command, &'static str); 3] {
    let y = if fixed { 790 } else { 350 };
    let width = (WIDTH as i32 - 128) / 3;
    let commands = if fixed {
        [Command::SaveFixed, Command::ResetFixed, Command::ClearFixed]
    } else {
        [
            Command::SaveReflow,
            Command::ResetReflow,
            Command::ClearReflow,
        ]
    };
    std::array::from_fn(|i| {
        (
            Rect::new(48 + i as i32 * (width + 16), y, width, 108),
            commands[i],
            ["Save current", "Reset book", "Clear defaults"][i],
        )
    })
}
impl Reader {
    fn default_command_available(&self, command: Command) -> bool {
        if self.document_defaults.problem.is_some()
            && !matches!(command, Command::ResetReflow | Command::ResetFixed)
        {
            return false;
        }
        match command {
            Command::SaveReflow | Command::ResetReflow => self.active.is_some(),
            Command::SaveFixed | Command::ResetFixed => self.fixed.is_some(),
            Command::ClearReflow => self.document_defaults.templates.reflow.is_some(),
            Command::ClearFixed => self.document_defaults.templates.fixed.is_some(),
        }
    }
    pub(super) fn draw_document_defaults(&self, canvas: &mut Canvas) {
        canvas.fill(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), 255);
        header(canvas, "Document defaults", "Done");
        for fixed in [false, true] {
            let top = if fixed { 590 } else { 150 };
            canvas.text(
                Rect::new(40, top, 1184, 64),
                if fixed {
                    "Fixed-page documents"
                } else {
                    "Reflowable books"
                },
                BODY.bold(),
            );
            canvas.card(Rect::new(40, top + 80, 1184, 260));
            let summary = if fixed {
                self.document_defaults
                    .templates
                    .fixed
                    .map_or("Device page defaults".into(), |_| {
                        "Saved page defaults for new documents".into()
                    })
            } else {
                self.document_defaults
                    .templates
                    .reflow
                    .as_ref()
                    .map_or_else(
                        || "Device text defaults".into(),
                        |p| {
                            format!(
                                "Saved text defaults · {} · {}",
                                p.logical_font_size.map_or_else(
                                    || format!("{} px", p.font_size.unwrap_or(42.)),
                                    |n| format!("{n} logical")
                                ),
                                if p.scroll { "Scroll" } else { "Pages" }
                            )
                        },
                    )
            };
            canvas.text(Rect::new(64, top + 100, 1136, 80), &summary, CAPTION);
            for (rect, command, label) in controls(fixed) {
                let available = self.default_command_available(command);
                if available {
                    canvas.button(rect, label, false, false);
                } else {
                    canvas.panel(rect, 16, 2, 255);
                    canvas.text(
                        Rect::new(rect.x, rect.y + 29, rect.w, 50),
                        label,
                        BODY.center().ink(130),
                    );
                }
                let pressed = available && canvas.is_control_pressed(rect);
                canvas.icon(
                    Rect::new(rect.x + 16, rect.y + 36, 36, 36),
                    bar_icons::action_icon(&Action::DocumentDefaults(command)),
                    if pressed {
                        255
                    } else if available {
                        0
                    } else {
                        178
                    },
                );
            }
        }
        canvas.text(Rect::new(48,980,1168,180),"Save current applies to new books only. Reset book applies the saved defaults to the open book. Existing reading positions and annotations stay in the book.",BODY.lines(3));
        canvas.text(Rect::new(48,1190,1168,140),"Fixed-page defaults include AutoTurn overlap and direction, but exclude crop and the current view offset. Reset keeps this document's crop and places its view within the page bounds. Clear defaults restores device settings for future books.",CAPTION.lines(3));
        if let Some(message) = self
            .document_defaults
            .problem
            .as_ref()
            .or(self.notice.as_ref())
        {
            canvas.text(Rect::new(48, 1390, 1168, 180), message, CAPTION.lines(4));
        }
    }
    pub(super) fn document_defaults_input(&mut self, input: Input) -> Option<Change> {
        if self.screen != Screen::DocumentDefaults {
            return None;
        }
        let Input::Tap(x, y) = input else {
            return None;
        };
        if DONE.contains(x, y) {
            self.show(Screen::ReadingControls);
            return Some(Change::Partial);
        }
        for fixed in [false, true] {
            for (rect, command, _) in controls(fixed) {
                if rect.contains(x, y) && self.default_command_available(command) {
                    return Some(match self.activate_document_defaults(command) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    });
                }
            }
        }
        Some(Change::None)
    }
}
#[cfg(test)]
mod tests;
