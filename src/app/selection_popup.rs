//! Small, anchored actions for the selection already painted on the reading page.

use primitive_app::{Change, WIDTH};
use primitive_ui::{CAPTION, Canvas, Editor, HAIRLINE, Rect, TextStyle};

use super::{Reader, Result, Screen, SelectionSearchOrigin};
use crate::highlight_style::{HighlightColor, HighlightStyle};
use crate::reading_controls::{SelectionLongPressAction, SelectionPopupPosition};
use crate::session::{DONE, PAGE_HEIGHT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HighlightPromptStage {
    Style,
    Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectionAction {
    Highlight,
    Copy,
    Note,
    Dictionary,
    Wikipedia,
    Translate,
    Find,
    Chapter,
    Html,
    Hyphenate,
    Cancel,
    PromptStyle(HighlightStyle),
    PromptColor(HighlightColor),
    CancelPrompt,
}

impl SelectionAction {
    const ALL: [Self; 11] = [
        Self::Highlight,
        Self::Copy,
        Self::Note,
        Self::Dictionary,
        Self::Wikipedia,
        Self::Translate,
        Self::Find,
        Self::Chapter,
        Self::Html,
        Self::Hyphenate,
        Self::Cancel,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Highlight => "Highlight",
            Self::Copy => "Copy",
            Self::Note => "Note",
            Self::Dictionary => "Dictionary",
            Self::Wikipedia => "Wikipedia",
            Self::Translate => "Translate",
            Self::Find => "Find",
            Self::Chapter => "Chapter",
            Self::Html => "View HTML",
            Self::Hyphenate => "Hyphenate",
            Self::Cancel => "Cancel",
            Self::PromptStyle(style) => style.label(),
            Self::PromptColor(color) => color.label(),
            Self::CancelPrompt => "Cancel",
        }
    }
}

pub(super) struct SelectionPopup {
    rect: Rect,
    large: bool,
    custom_chapters: bool,
    hyphenate: bool,
    html: bool,
    select_mode: bool,
    prompt_stage: Option<HighlightPromptStage>,
    pending_style: Option<HighlightStyle>,
    pending_color: Option<HighlightColor>,
}

impl SelectionPopup {
    fn new(
        bounds: Option<Rect>,
        position: SelectionPopupPosition,
        large: bool,
        custom_chapters: bool,
        hyphenate: bool,
        html: bool,
        select_mode: bool,
        prompt_stage: Option<HighlightPromptStage>,
        pending_style: Option<HighlightStyle>,
        pending_color: Option<HighlightColor>,
    ) -> Self {
        if let Some(stage) = prompt_stage {
            let width = 1040.min(WIDTH as i32 - 48);
            let height = match stage {
                HighlightPromptStage::Style => 340,
                HighlightPromptStage::Color => 520,
            };
            let edge = 16;
            let y = match position {
                SelectionPopupPosition::Top => edge,
                SelectionPopupPosition::Center => (PAGE_HEIGHT as i32 - height) / 2,
                SelectionPopupPosition::Bottom => PAGE_HEIGHT as i32 - height - edge,
                SelectionPopupPosition::Highlight => bounds
                    .and_then(|bounds| {
                        let below = bounds.bottom() + 24;
                        let above = bounds.y - height - 24;
                        if below + height <= PAGE_HEIGHT as i32 - edge {
                            Some(below)
                        } else if above >= edge {
                            Some(above)
                        } else {
                            None
                        }
                    })
                    .unwrap_or((PAGE_HEIGHT as i32 - height) / 2),
            };
            return Self {
                rect: Rect::new((WIDTH as i32 - width) / 2, y.max(edge), width, height),
                large,
                custom_chapters,
                hyphenate,
                html,
                select_mode,
                prompt_stage,
                pending_style,
                pending_color,
            };
        }
        if select_mode {
            let width = 700.min(WIDTH as i32 - 72);
            let height = 180;
            return Self {
                rect: Rect::new(
                    (WIDTH as i32 - width) / 2,
                    PAGE_HEIGHT as i32 - height - 16,
                    width,
                    height,
                ),
                large,
                custom_chapters,
                hyphenate,
                html,
                select_mode,
                prompt_stage,
                pending_style,
                pending_color,
            };
        }
        let height = if large { 260 } else { 224 };
        let width = (WIDTH as i32 - 72).min(950);
        let page_bottom = PAGE_HEIGHT as i32;
        // Leave space for the next line and for the canvas's popup border.
        let gap = 56;
        let edge = 16;
        let fixed_y = match position {
            SelectionPopupPosition::Top => Some(edge),
            SelectionPopupPosition::Center => Some((page_bottom - height) / 2),
            SelectionPopupPosition::Bottom => Some(page_bottom - height - edge),
            SelectionPopupPosition::Highlight => None,
        };
        if let Some(y) = fixed_y {
            return Self {
                rect: Rect::new((WIDTH as i32 - width) / 2, y, width, height),
                large,
                custom_chapters,
                hyphenate,
                html,
                select_mode,
                prompt_stage,
                pending_style,
                pending_color,
            };
        }
        if let Some(bounds) = bounds {
            let below = bounds.bottom() + gap;
            let above = bounds.y - height - gap;
            let y = if below + height <= page_bottom - edge {
                Some(below)
            } else if above >= edge {
                Some(above)
            } else {
                None
            };
            if let Some(y) = y {
                let x =
                    (bounds.x + bounds.w / 2 - width / 2).clamp(edge, WIDTH as i32 - width - edge);
                return Self {
                    rect: Rect::new(x, y, width, height),
                    large,
                    custom_chapters,
                    hyphenate,
                    html,
                    select_mode,
                    prompt_stage,
                    pending_style,
                    pending_color,
                };
            }
        }

        // A page-spanning selection can leave no clear location for the popup in the text.
        // Put its actions in the footer alongside Done and leave every selected word visible.
        let width = width.min(DONE.x - 36);
        Self {
            rect: Rect::new(16, page_bottom + 12, width, 50),
            large,
            custom_chapters,
            hyphenate,
            html,
            select_mode,
            prompt_stage,
            pending_style,
            pending_color,
        }
    }

    fn buttons(&self) -> impl Iterator<Item = (SelectionAction, Rect)> + '_ {
        let inner = self.rect.inset(5);
        let floating = self.rect.y < PAGE_HEIGHT as i32;
        let actions: Vec<_> = if let Some(stage) = self.prompt_stage {
            match stage {
                HighlightPromptStage::Style => HighlightStyle::ALL
                    .into_iter()
                    .map(SelectionAction::PromptStyle)
                    .chain([SelectionAction::CancelPrompt])
                    .collect(),
                HighlightPromptStage::Color => HighlightColor::ALL
                    .into_iter()
                    .map(SelectionAction::PromptColor)
                    .chain([SelectionAction::CancelPrompt])
                    .collect(),
            }
        } else if self.select_mode {
            vec![
                SelectionAction::Cancel,
                SelectionAction::Copy,
                SelectionAction::Highlight,
            ]
        } else {
            SelectionAction::ALL
                .into_iter()
                .filter(|action| *action != SelectionAction::Chapter || self.custom_chapters)
                .filter(|action| *action != SelectionAction::Hyphenate || self.hyphenate)
                .filter(|action| *action != SelectionAction::Html || self.html)
                .collect()
        };
        let count = actions.len() as i32;
        let columns = if let Some(stage) = self.prompt_stage {
            match stage {
                HighlightPromptStage::Style => 3,
                HighlightPromptStage::Color => 3,
            }
        } else if self.select_mode {
            count
        } else if floating {
            (count + 1) / 2
        } else {
            count
        };
        let cell_top = if self.prompt_stage.is_some() {
            inner.y + 48
        } else if self.select_mode {
            inner.y + 44
        } else {
            inner.y
        };
        let cell_h = if self.prompt_stage.is_some() {
            (inner.bottom() - cell_top) / ((count + columns - 1) / columns)
        } else if self.select_mode {
            (inner.bottom() - cell_top).max(1)
        } else if floating {
            inner.h / 2
        } else {
            inner.h
        };
        actions.into_iter().enumerate().map(move |(index, action)| {
            let index = index as i32;
            let row = index / columns;
            let cols = (count - row * columns).min(columns);
            let col = index % columns;
            let left = inner.x + inner.w * col / cols;
            let right = inner.x + inner.w * (col + 1) / cols;
            (
                action,
                Rect::new(left, cell_top + row * cell_h, right - left, cell_h),
            )
        })
    }

    fn action_at(&self, x: f32, y: f32) -> Option<SelectionAction> {
        self.buttons()
            .find_map(|(action, rect)| rect.contains(x, y).then_some(action))
    }

    fn draw(&self, canvas: &mut Canvas, notice: Option<&str>) {
        let floating = self.rect.y < PAGE_HEIGHT as i32;
        canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
        if let Some(stage) = self.prompt_stage {
            canvas.popup(self.rect);
            canvas.text(
                Rect::new(self.rect.x + 20, self.rect.y + 6, self.rect.w - 40, 40),
                match stage {
                    HighlightPromptStage::Style => "Highlight · choose style",
                    HighlightPromptStage::Color => "Highlight · choose color",
                },
                CAPTION.bold(),
            );
            for (action, rect) in self.buttons() {
                match action {
                    SelectionAction::PromptColor(color) => {
                        let selected = self.pending_color == Some(color);
                        canvas.button(rect, "", selected, false);
                        let selected = selected != canvas.is_control_pressed(rect);
                        let swatch = Rect::new(rect.x + 22, rect.y + 24, 44, 44);
                        canvas.panel(swatch, 4, 2, color.gray().unwrap_or(204));
                        canvas.text(
                            Rect::new(swatch.right() + 12, rect.y + 12, rect.w - 88, rect.h - 24),
                            color.label(),
                            TextStyle::new(26)
                                .bold()
                                .center()
                                .ink(if selected { 255 } else { 0 }),
                        );
                    }
                    SelectionAction::PromptStyle(style) => canvas.button(
                        rect,
                        action.label(),
                        self.pending_style == Some(style),
                        false,
                    ),
                    _ => canvas.button(rect, action.label(), false, false),
                }
            }
            canvas.button(DONE, "Done", false, false);
            return;
        }
        if self.select_mode {
            canvas.popup(self.rect);
            canvas.text(
                Rect::new(self.rect.x + 16, self.rect.y + 6, self.rect.w - 32, 32),
                notice.unwrap_or("Tap a word to extend · Highlight to save · Copy text"),
                CAPTION,
            );
            for (action, rect) in self.buttons() {
                canvas.button(
                    rect,
                    action.label(),
                    action == SelectionAction::Highlight,
                    false,
                );
            }
            canvas.button(DONE, "Done", false, false);
            return;
        }
        if floating {
            canvas.text(
                Rect::new(38, PAGE_HEIGHT as i32 + 12, DONE.x - 62, 48),
                notice.unwrap_or("Tap another word to extend"),
                CAPTION,
            );
        }
        // The balanced border also tells Canvas to cover the page raster under this popup.
        canvas.popup(self.rect);
        let size = if !floating {
            23
        } else if self.large {
            34
        } else {
            28
        };
        for (action, rect) in self.buttons() {
            if rect.x > self.rect.x + 5 {
                canvas.fill(Rect::new(rect.x, rect.y + 10, 1, rect.h - 20), HAIRLINE);
            }
            if floating && rect.y > self.rect.y + 5 {
                canvas.fill(Rect::new(rect.x + 10, rect.y, rect.w - 20, 1), HAIRLINE);
            }
            let pressed = canvas.control_pressed(rect, false);
            let ink = if pressed && action == SelectionAction::Html {
                255
            } else {
                0
            };
            if pressed {
                canvas.fill(
                    rect.inset(2),
                    if action == SelectionAction::Html {
                        0
                    } else {
                        218
                    },
                );
            }
            let text_y = if floating {
                use super::bar_icons;
                let icon = match action {
                    SelectionAction::Highlight => bar_icons::HIGHLIGHT,
                    SelectionAction::Copy => bar_icons::COPY,
                    SelectionAction::Note => bar_icons::PAGE,
                    SelectionAction::Dictionary | SelectionAction::Chapter => bar_icons::BOOK,
                    SelectionAction::Wikipedia => bar_icons::LINK,
                    SelectionAction::Hyphenate | SelectionAction::Translate => bar_icons::TEXT,
                    SelectionAction::Html => bar_icons::CODE,
                    SelectionAction::Find => bar_icons::SEARCH,
                    SelectionAction::Cancel => bar_icons::BACK,
                    SelectionAction::PromptStyle(_) | SelectionAction::PromptColor(_) => {
                        bar_icons::HIGHLIGHT
                    }
                    SelectionAction::CancelPrompt => bar_icons::BACK,
                };
                canvas.icon(
                    Rect::new(rect.x + (rect.w - 40) / 2, rect.y + 9, 40, 40),
                    icon,
                    ink,
                );
                rect.y + 55
            } else {
                rect.y + (rect.h - size) / 2
            };
            canvas.text(
                Rect::new(rect.x, text_y, rect.w, size + 9),
                if floating || action != SelectionAction::Html {
                    action.label()
                } else {
                    "HTML"
                },
                TextStyle::new(size).bold().center().ink(ink),
            );
        }
        canvas.button(DONE, "Done", false, false);
    }
}

impl Reader {
    fn copy_selection(&mut self) -> Result<()> {
        let text = self
            .selection
            .as_ref()
            .map(|selection| selection.text.as_str())
            .or_else(|| {
                self.pdf_selection
                    .as_ref()
                    .map(|selection| selection.text())
            })
            .ok_or_else(|| std::io::Error::other("There is no selected text to copy"))?;
        let text = cleanup_selected_text(text);
        if text.is_empty() {
            return Err("There is no selected text to copy".into());
        }
        self.clipboard_text = Some(text);
        self.selection = None;
        self.pdf_selection = None;
        self.selection_mode = false;
        self.notice = Some("Selection copied to clipboard".into());
        Ok(())
    }

    pub(super) fn handle_text_long_press(&mut self) -> Change {
        self.handle_text_long_press_with_force_ask(false)
    }

    pub(super) fn handle_text_long_press_with_force_ask(&mut self, force_ask: bool) -> Change {
        let single_word = self
            .selection
            .as_ref()
            .map(|selection| selection.text.as_str())
            .or_else(|| {
                self.pdf_selection
                    .as_ref()
                    .map(|selection| selection.text())
            })
            .is_some_and(|text| text.split_whitespace().count() == 1);
        let action = if force_ask {
            SelectionLongPressAction::Ask
        } else if single_word && self.controls.dictionary_on_single_word.unwrap_or(true) {
            SelectionLongPressAction::Dictionary
        } else {
            self.controls.selection_long_press_action
        };
        if self.cjk_selection_lookup.is_some()
            && !matches!(
                action,
                SelectionLongPressAction::Ask
                    | SelectionLongPressAction::Select
                    | SelectionLongPressAction::Nothing
            )
        {
            self.pending_selection_action = Some(action);
            return Change::Partial;
        }
        self.apply_selection_long_press_action(action)
    }

    pub(super) fn apply_selection_long_press_action(
        &mut self,
        action: SelectionLongPressAction,
    ) -> Change {
        if action == SelectionLongPressAction::Ask {
            return Change::Partial;
        }
        if action == SelectionLongPressAction::Select {
            self.selection_mode = true;
            self.notice = None;
            return Change::Partial;
        }
        if action == SelectionLongPressAction::Nothing {
            self.selection = None;
            self.pdf_selection = None;
            self.selection_mode = false;
            self.pending_selection_action = None;
            self.cjk_selection_lookup = None;
            self.notice = None;
            return Change::Partial;
        }
        let result = match action {
            SelectionLongPressAction::Ask
            | SelectionLongPressAction::Select
            | SelectionLongPressAction::Nothing => unreachable!(),
            SelectionLongPressAction::Highlight => self.begin_highlight(),
            SelectionLongPressAction::Note => {
                self.editor = Editor::new(128);
                self.show(Screen::Note);
                Ok(())
            }
            SelectionLongPressAction::Translate => self.prepare_online(false, false),
            SelectionLongPressAction::Wikipedia => self.prepare_online(true, false),
            SelectionLongPressAction::Dictionary => self.look_up_selection(),
            SelectionLongPressAction::Search => self.find_selection(SelectionSearchOrigin::Reading),
        };
        match result {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    pub(super) fn extend_reflow_selection(&mut self, x: f32, y: f32) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        let Some(selection) = &self.selection else {
            return false;
        };
        let Some(word) = active.reading.word_at(x, y) else {
            return false;
        };
        if word.start.book != selection.start.book
            || word.start.chapter != selection.start.chapter
            || word.start.revision != selection.start.revision
        {
            self.notice = Some("Select a word in this chapter".into());
            return false;
        }
        let start = if (word.start.block, word.start.offset)
            < (selection.start.block, selection.start.offset)
        {
            &word.start
        } else {
            &selection.start
        };
        let end = if (word.end.block, word.end.offset) > (selection.end.block, selection.end.offset)
        {
            &word.end
        } else {
            &selection.end
        };
        let extended = active.reading.probe.select_range(start, end);
        if let Some(extended) = extended {
            self.selection = Some(extended);
            self.notice = None;
            true
        } else {
            false
        }
    }

    pub(super) fn selection_bounds(&self) -> Option<Rect> {
        if let (Some(fixed), Some(selection)) = (&self.fixed, &self.pdf_selection) {
            fixed.reading.selection_bounds(selection)
        } else if let (Some(active), Some(selection)) = (&self.active, &self.selection) {
            active
                .reading
                .highlight_rects(selection)
                .into_iter()
                .map(|rect| Rect::new(rect.x, rect.y, rect.width, rect.height))
                .reduce(|left, right| {
                    let x = left.x.min(right.x);
                    let y = left.y.min(right.y);
                    Rect::new(
                        x,
                        y,
                        left.right().max(right.right()) - x,
                        left.bottom().max(right.bottom()) - y,
                    )
                })
        } else {
            None
        }
    }

    fn selection_popup(&self) -> Option<SelectionPopup> {
        (self.selection.is_some() || self.pdf_selection.is_some()).then(|| {
            SelectionPopup::new(
                self.selection_bounds(),
                self.controls.selection_popup_position,
                self.controls.large_toolbar,
                self.custom_contents_enabled(),
                self.hyphenation_word().is_some(),
                self.can_view_selection_html(),
                self.selection_mode,
                self.highlight_prompt_stage,
                self.pending_highlight_style,
                self.pending_highlight_color,
            )
        })
    }

    fn begin_highlight(&mut self) -> Result<()> {
        use crate::reading_controls::HighlightPrompt;

        self.pending_highlight_style = None;
        self.pending_highlight_color = None;
        match self.controls.highlight_prompt {
            HighlightPrompt::None => self.commit_highlight(),
            HighlightPrompt::Style => {
                self.pending_highlight_style = Some(self.effective_highlight_style());
                self.highlight_prompt_stage = Some(HighlightPromptStage::Style);
                Ok(())
            }
            HighlightPrompt::Color => {
                self.pending_highlight_color = Some(self.effective_highlight_color());
                self.highlight_prompt_stage = Some(HighlightPromptStage::Color);
                Ok(())
            }
            HighlightPrompt::StyleAndColor => {
                self.pending_highlight_style = Some(self.effective_highlight_style());
                self.highlight_prompt_stage = Some(HighlightPromptStage::Style);
                Ok(())
            }
        }
    }

    fn commit_highlight(&mut self) -> Result<()> {
        let result = if self.pdf_selection.is_some() {
            self.save_pdf_highlight(None)
        } else {
            self.save_highlight(None)
        };
        if result.is_ok() {
            self.highlight_prompt_stage = None;
            self.pending_highlight_style = None;
            self.pending_highlight_color = None;
        }
        result
    }

    fn choose_prompt_style(&mut self, style: crate::highlight_style::HighlightStyle) -> Result<()> {
        use crate::reading_controls::HighlightPrompt;

        self.pending_highlight_style = Some(style);
        if self.controls.highlight_prompt == HighlightPrompt::StyleAndColor
            && style != crate::highlight_style::HighlightStyle::Invert
        {
            self.pending_highlight_color = Some(self.effective_highlight_color());
            self.highlight_prompt_stage = Some(HighlightPromptStage::Color);
            Ok(())
        } else {
            self.commit_highlight()
        }
    }

    fn cancel_highlight_prompt(&mut self) {
        self.highlight_prompt_stage = None;
        self.pending_highlight_style = None;
        self.pending_highlight_color = None;
        self.notice = None;
    }

    pub(super) fn selection_popup_input(&mut self, x: f32, y: f32) -> Option<Change> {
        if DONE.contains(x, y) {
            return Some(self.exit());
        }
        let popup = self.selection_popup()?;
        if !popup.rect.contains(x, y) {
            return None;
        }
        let result = match popup.action_at(x, y) {
            Some(SelectionAction::Highlight) => self.begin_highlight(),
            Some(SelectionAction::Copy) => self.copy_selection(),
            Some(SelectionAction::PromptStyle(style)) => self.choose_prompt_style(style),
            Some(SelectionAction::PromptColor(color)) => {
                self.pending_highlight_color = Some(color);
                self.commit_highlight()
            }
            Some(SelectionAction::CancelPrompt) => {
                self.cancel_highlight_prompt();
                Ok(())
            }
            Some(SelectionAction::Note) => {
                self.editor = Editor::new(128);
                self.show(Screen::Note);
                Ok(())
            }
            Some(SelectionAction::Dictionary) => self.look_up_selection(),
            Some(SelectionAction::Wikipedia) => self.prepare_online(true, false),
            Some(SelectionAction::Translate) => self.prepare_online(false, false),
            Some(SelectionAction::Find) => self.find_selection(SelectionSearchOrigin::Reading),
            Some(SelectionAction::Hyphenate) => self.begin_hyphenation(),
            Some(SelectionAction::Chapter) => self.edit_selected_contents_chapter(),
            Some(SelectionAction::Html) => {
                return Some(match self.view_selection_html() {
                    Ok(()) => Change::Full,
                    Err(error) => self.report(error),
                });
            }
            Some(SelectionAction::Cancel) => {
                self.selection = None;
                self.pdf_selection = None;
                self.selection_mode = false;
                self.notice = None;
                Ok(())
            }
            None => return Some(Change::None),
        };
        Some(match result {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    pub(super) fn draw_selection_popup(&self, canvas: &mut Canvas) {
        if let Some(popup) = self.selection_popup() {
            popup.draw(canvas, self.notice.as_deref());
        }
    }

    #[cfg(test)]
    pub(super) fn selection_button(&self, action: SelectionAction) -> Rect {
        self.selection_popup()
            .unwrap()
            .buttons()
            .find_map(|(candidate, rect)| (candidate == action).then_some(rect))
            .unwrap()
    }

    #[cfg(test)]
    pub(super) fn selection_popup_rect(&self) -> Rect {
        self.selection_popup().unwrap().rect
    }

    #[cfg(test)]
    pub(super) fn selection_popup_action_count(&self) -> usize {
        self.selection_popup().unwrap().buttons().count()
    }
}

fn cleanup_selected_text(text: &str) -> String {
    let mut cleaned = String::new();
    let mut chars = text.trim_matches(char::is_whitespace).chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\n' {
            while cleaned.chars().last().is_some_and(char::is_whitespace) {
                cleaned.pop();
            }
            cleaned.push('\n');
            while chars.peek().is_some_and(|next| next.is_whitespace()) {
                chars.next();
            }
        } else {
            cleaned.push(ch);
        }
    }

    let mut collapsed = String::with_capacity(cleaned.len());
    let mut chars = cleaned.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            let mut run = 1;
            while chars.peek().is_some_and(|next| next.is_whitespace()) {
                chars.next();
                run += 1;
            }
            collapsed.push(if run > 1 { ' ' } else { ch });
        } else {
            collapsed.push(ch);
        }
    }
    collapsed
}

#[cfg(test)]
mod copy_tests {
    use super::cleanup_selected_text;

    #[test]
    fn selected_text_cleanup_matches_koreader_whitespace_rules() {
        assert_eq!(cleanup_selected_text("  alpha  beta  "), "alpha beta");
        assert_eq!(cleanup_selected_text("  alpha \n  beta  "), "alpha\nbeta");
        assert_eq!(cleanup_selected_text("alpha\n\nbeta"), "alpha\nbeta");
        assert_eq!(cleanup_selected_text("\t  \n  "), "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_avoids_selected_text_and_the_done_control() {
        for (bounds, large) in [
            (Rect::new(0, 12, 80, 45), false),
            (Rect::new(WIDTH as i32 - 100, 760, 100, 45), true),
            (
                Rect::new(0, 8, WIDTH as i32, PAGE_HEIGHT as i32 - 16),
                false,
            ),
        ] {
            let popup = SelectionPopup::new(
                Some(bounds),
                SelectionPopupPosition::Highlight,
                large,
                true,
                false,
                false,
                false,
                None,
                None,
                None,
            );
            let rect = popup.rect;
            assert!(rect.x >= 16 && rect.right() <= WIDTH as i32 - 16);
            assert!(rect.bottom() <= DONE.y || rect.right() < DONE.x);
            assert!(rect.y >= bounds.bottom() || rect.bottom() <= bounds.y);
            assert!(rect.bottom() <= primitive_app::HEIGHT as i32 - 10);
            for (action, button) in popup.buttons() {
                assert_eq!(
                    popup.action_at(
                        (button.x + button.w / 2) as f32,
                        (button.y + button.h / 2) as f32
                    ),
                    Some(action),
                );
            }
        }
    }

    #[test]
    fn fixed_popup_positions_match_the_reading_page_edges_and_center() {
        let height = 224;
        let width = 950;
        for (position, y) in [
            (SelectionPopupPosition::Top, 16),
            (
                SelectionPopupPosition::Center,
                (PAGE_HEIGHT as i32 - height) / 2,
            ),
            (
                SelectionPopupPosition::Bottom,
                PAGE_HEIGHT as i32 - height - 16,
            ),
        ] {
            let popup = SelectionPopup::new(
                Some(Rect::new(400, 700, 120, 50)),
                position,
                false,
                true,
                false,
                false,
                false,
                None,
                None,
                None,
            );
            assert_eq!(
                popup.rect,
                Rect::new((WIDTH as i32 - width) / 2, y, width, height)
            );
            assert!(
                popup
                    .buttons()
                    .all(|(_, rect)| popup.rect.contains(rect.x as f32, rect.y as f32))
            );
        }
    }

    #[test]
    fn select_mode_has_full_size_cancel_copy_and_highlight_buttons() {
        let popup = SelectionPopup::new(
            Some(Rect::new(420, 650, 180, 80)),
            SelectionPopupPosition::Center,
            false,
            true,
            true,
            false,
            true,
            None,
            None,
            None,
        );
        assert_eq!(popup.rect.y, PAGE_HEIGHT as i32 - 196);
        assert_eq!(popup.buttons().count(), 3);
        assert_eq!(
            popup
                .buttons()
                .map(|(action, _)| action)
                .collect::<Vec<_>>(),
            [
                SelectionAction::Cancel,
                SelectionAction::Copy,
                SelectionAction::Highlight
            ]
        );
        for (action, rect) in popup.buttons() {
            assert_eq!(
                popup.action_at((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32,),
                Some(action)
            );
            assert!(rect.w >= 100 && rect.h >= 100);
        }
        assert!(popup.rect.bottom() < PAGE_HEIGHT as i32);
    }

    #[test]
    fn highlight_prompt_buttons_remain_full_size_inside_the_page() {
        for (stage, minimum_count) in [
            (HighlightPromptStage::Style, HighlightStyle::ALL.len() + 1),
            (HighlightPromptStage::Color, HighlightColor::ALL.len() + 1),
        ] {
            let popup = SelectionPopup::new(
                Some(Rect::new(450, 700, 180, 60)),
                SelectionPopupPosition::Highlight,
                false,
                true,
                false,
                false,
                false,
                Some(stage),
                None,
                None,
            );
            assert_eq!(popup.buttons().count(), minimum_count);
            assert!(popup.rect.x >= 16 && popup.rect.right() <= WIDTH as i32 - 16);
            assert!(popup.rect.y >= 16 && popup.rect.bottom() <= PAGE_HEIGHT as i32 - 16);
            for (action, rect) in popup.buttons() {
                assert!(rect.w >= 100 && rect.h >= 100, "{action:?}: {rect:?}");
                assert!(popup.rect.contains(rect.x as f32, rect.y as f32));
                assert_eq!(
                    popup.action_at((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32),
                    Some(action)
                );
            }
        }
    }
}
