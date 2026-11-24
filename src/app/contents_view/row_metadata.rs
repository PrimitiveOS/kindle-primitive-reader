//! Lay out Contents metadata in the title row without changing its tap target.
use super::{Action, Canvas, Item, Reader, Rect, TextStyle};
use crate::app::contents_position::ContentsNumbers;
use crate::reading_controls::ReadingControls;

pub(super) struct RowTypography {
    pub title_size: i32,
    pub title_bold: bool,
    pub info_size: i32,
    pub ink: u8,
    pub info_ink: u8,
}

impl Reader {
    pub(in crate::app) fn contents_metadata_settings_items(&self) -> [Item; 2] {
        let lengths = self.controls.contents_chapter_lengths;
        let dots = self.controls.contents_dot_leaders.unwrap_or(true);
        let on_off = |enabled| if enabled { "On" } else { "Off" };
        [
            Item::new(
                "Show chapter length",
                Action::SetReadingControls(ReadingControls {
                    contents_chapter_lengths: !lengths,
                    ..self.controls
                }),
            )
            .detail(on_off(lengths)),
            Item::new(
                "Dot leaders",
                Action::SetReadingControls(ReadingControls {
                    contents_dot_leaders: Some(!dots),
                    ..self.controls
                }),
            )
            .detail(on_off(dots)),
        ]
    }

    pub(super) fn draw_contents_title(
        &self,
        canvas: &mut Canvas,
        rect: Rect,
        title: &str,
        numbers: Option<&ContentsNumbers>,
        typeface: RowTypography,
    ) {
        let RowTypography {
            title_size,
            title_bold,
            info_size,
            ink,
            info_ink,
        } = typeface;
        let mut title_style = TextStyle::new(title_size).ink(ink);
        if title_bold {
            title_style = title_style.bold();
        }
        let Some(numbers) = numbers else {
            canvas.text(rect, title, title_style);
            return;
        };
        let length = if self.controls.contents_chapter_lengths {
            numbers.chapter_length.map(|length| format!("({length})"))
        } else {
            None
        };
        let label = numbers.page_label.as_deref();
        if length.is_none() && label.is_none() {
            canvas.text(rect, title, title_style);
            return;
        }

        // Metadata shares the title baseline. Reserve a bounded part of the
        // row so unusually long publisher labels cannot hide the heading.
        let spacing = (info_size / 3).max(4);
        let measure = |canvas: &mut Canvas, text: &str| {
            canvas.measure(text, info_size).min(rect.w / 3).max(0)
        };
        let length_width = length.as_deref().map_or(0, |text| measure(canvas, text));
        let label_width = label.map_or(0, |text| measure(canvas, text));
        let label_x = rect.right() - label_width;
        let title_limit = (rect.w
            - length_width
            - label_width
            - (i32::from(length.is_some()) + i32::from(label.is_some())) * spacing)
            .max(0);
        let title_width = canvas.measure_with_weight(title, title_size, title_bold);
        let fitted_width = title_width.min(title_limit);
        canvas.text(
            Rect::new(rect.x, rect.y, title_limit, rect.h),
            title,
            title_style,
        );
        let info_y = rect.y + (title_size - info_size).max(0);
        let info_height = rect.bottom() - info_y;
        let info_style = TextStyle::new(info_size).ink(info_ink);
        let length_x = rect.x + fitted_width + spacing;
        if let Some(length) = length {
            canvas.text(
                Rect::new(length_x, info_y, length_width, info_height),
                &length,
                info_style,
            );
        }
        if let Some(label) = label {
            canvas.text(
                Rect::new(label_x, info_y, label_width, info_height),
                label,
                info_style.right(),
            );
            if self.controls.contents_dot_leaders.unwrap_or(true) && title_width <= title_limit {
                let dots_start = length_x + length_width + i32::from(length_width > 0) * spacing;
                let available = label_x - dots_start - spacing;
                // Round a run of glyphs rather than accumulating the rounding
                // of a single advance over a long leader.
                let step = canvas.measure("................", info_size).max(1);
                let count = available * 16 / step;
                if count >= 3 {
                    canvas.text(
                        Rect::new(dots_start, info_y, available, info_height),
                        &".".repeat(count as usize),
                        info_style,
                    );
                }
            }
        }
    }
}
