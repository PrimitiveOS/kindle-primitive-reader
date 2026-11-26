//! Saved mark presentation shared by the fixed and flexible bookmark lists.
use primitive_ui::{Canvas, Rect, TextStyle};

use super::super::{Action, Item, Reader, highlight_palette};
use crate::highlight_style::HighlightColor;
use crate::reading_controls::ReadingControls;

impl Reader {
    pub(in crate::app) fn bookmark_appearance_settings_items(&self) -> [Item; 3] {
        let controls = self.controls;
        let switch_label = |enabled| if enabled { "On" } else { "Off" };
        let default_action = if controls.bookmark_show_highlight_colors {
            Action::SetReadingControls(ReadingControls {
                bookmark_show_default_color: !controls.bookmark_show_default_color,
                ..controls
            })
        } else {
            Action::Heading
        };
        [
            Item::new(
                "Show separator between items",
                Action::SetReadingControls(ReadingControls {
                    bookmark_list_separators: !controls.bookmark_list_separators,
                    ..controls
                }),
            )
            .detail(switch_label(controls.bookmark_list_separators)),
            Item::new(
                "Show highlight colors",
                Action::SetReadingControls(ReadingControls {
                    bookmark_show_highlight_colors: !controls.bookmark_show_highlight_colors,
                    ..controls
                }),
            )
            .detail(switch_label(controls.bookmark_show_highlight_colors)),
            Item::new("Also show default highlight color", default_action).detail(
                if controls.bookmark_show_highlight_colors {
                    switch_label(controls.bookmark_show_default_color)
                } else {
                    "Show highlight colors first"
                },
            ),
        ]
    }

    pub(super) fn bookmark_row_color(&self, action: &Action) -> Option<HighlightColor> {
        if !self.controls.bookmark_show_highlight_colors {
            return None;
        }
        if let Some((color, default)) = self.bookmark_browser_row_colors(action) {
            return (self.controls.bookmark_show_default_color || color != default)
                .then_some(color);
        }
        let index = match *action {
            Action::Highlight(index) | Action::PdfHighlight(index) => index,
            _ => return None,
        };
        let color = self.mark_color(index)?;
        (self.controls.bookmark_show_default_color || color != self.effective_highlight_color())
            .then_some(color)
    }
}

pub(super) fn draw_caption(
    canvas: &mut Canvas,
    mut bounds: Rect,
    caption: &str,
    size: i32,
    ink: u8,
    color: Option<HighlightColor>,
) {
    if let Some(color) = color {
        // A name distinguishes colors whose E Ink tones are similar. Use the
        // picker sample and the existing caption line without repacking rows.
        let badge_size = size.min(24);
        let label_width = canvas.measure(color.label(), badge_size) + 4;
        let swatch_size = (bounds.h - 4).clamp(1, 22);
        let right = bounds.right();
        let x = right - label_width - 8 - swatch_size;
        highlight_palette::draw_swatch(
            canvas,
            Rect::new(
                x,
                bounds.y + (bounds.h - swatch_size) / 2,
                swatch_size,
                swatch_size,
            ),
            color,
        );
        let label_height = (badge_size * 13 + 5) / 10;
        canvas.text(
            Rect::new(
                right - label_width,
                bounds.y + (bounds.h - label_height) / 2,
                label_width,
                label_height,
            ),
            color.label(),
            TextStyle::new(badge_size).ink(ink),
        );
        bounds.w = x - bounds.x - 12;
    }
    canvas.text(
        bounds,
        caption,
        TextStyle::new(size).break_long_words().ink(ink),
    );
}
