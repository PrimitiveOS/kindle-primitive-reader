//! Measure custom footer items on either side of an optional expanding gap.
use super::{
    FOOTER_ICON_GAP, FOOTER_ICON_SIZE, FooterItem, FooterSymbolStyle, FooterTrackPosition, Reader,
    footer_item_icon,
};
use primitive_ui::{Canvas, Icon, Rect, TextStyle};

enum Fragment {
    Icon(Icon),
    Text { text: String, width: i32 },
}

impl Fragment {
    fn width(&self) -> i32 {
        match self {
            Self::Icon(_) => FOOTER_ICON_SIZE + FOOTER_ICON_GAP,
            Self::Text { width, .. } => *width,
        }
    }
}

#[derive(Default)]
struct Side {
    fragments: Vec<Fragment>,
    width: i32,
}

impl Side {
    fn push(&mut self, fragment: Fragment) {
        self.width += fragment.width();
        self.fragments.push(fragment);
    }

    fn push_text(&mut self, text: &mut String, measure: &mut impl FnMut(&str) -> i32) {
        if !text.is_empty() {
            self.push(Fragment::Text {
                width: measure(text),
                text: std::mem::take(text),
            });
        }
    }

    fn from_items(
        reader: &Reader,
        items: &[(FooterItem, String, bool)],
        before: Option<String>,
        after: Option<String>,
        measure: &mut impl FnMut(&str) -> i32,
    ) -> Self {
        let symbols = reader.controls.footer_symbol_style;
        let separator = reader.controls.footer_separator.text();
        let inverted = reader.page_turning_inverted();
        let mut side = Self::default();
        let mut has_item = before.is_some();
        let mut text = before.unwrap_or_default();
        let mut merged = false;
        for (item, value, merge) in items {
            if has_item && !merged && !merge {
                text.push_str(separator);
            }
            if symbols == FooterSymbolStyle::Icons
                && let Some(icon) = footer_item_icon(*item, inverted)
            {
                side.push_text(&mut text, measure);
                side.push(Fragment::Icon(icon));
            }
            text.push_str(symbols.prefix_for(*item, inverted));
            text.push_str(value);
            has_item = true;
            merged = *merge;
        }
        if let Some(after) = after {
            if has_item && !merged {
                text.push_str(separator);
            }
            text.push_str(&after);
        }
        side.push_text(&mut text, measure);
        side
    }

    fn draw(
        &self,
        canvas: &mut Canvas,
        mut x: i32,
        rect: Rect,
        style: TextStyle,
        font_file: Option<&str>,
    ) {
        for fragment in &self.fragments {
            match fragment {
                Fragment::Icon(icon) => {
                    let top = rect.y + (rect.h - FOOTER_ICON_SIZE) / 2;
                    canvas.icon(
                        Rect::new(x, top, FOOTER_ICON_SIZE, FOOTER_ICON_SIZE),
                        *icon,
                        0,
                    );
                }
                Fragment::Text { text, width } if *width > 0 => {
                    let text_rect = Rect::new(x, rect.y, *width, rect.h);
                    if let Some(file) = font_file {
                        canvas.text_with_font_file(text_rect, text, style, file);
                    } else {
                        canvas.text(text_rect, text, style);
                    }
                }
                Fragment::Text { .. } => {}
            }
            x += fragment.width();
        }
    }
}

pub(in crate::app) struct FillerLine {
    left: Side,
    right: Side,
    width: u32,
}

impl FillerLine {
    pub(in crate::app) const fn width(&self) -> u32 {
        self.width
    }

    pub(in crate::app) fn draw(
        &self,
        canvas: &mut Canvas,
        rect: Rect,
        style: TextStyle,
        font_file: Option<&str>,
    ) {
        self.left.draw(canvas, rect.x, rect, style, font_file);
        self.right.draw(
            canvas,
            rect.right() - self.right.width,
            rect,
            style,
            font_file,
        );
    }
}

impl Reader {
    pub(in crate::app) fn custom_footer_filler_line(
        &self,
        canvas: &mut Canvas,
        width: u32,
        font_file: Option<&str>,
    ) -> Option<FillerLine> {
        let controls = self.controls;
        if !self.custom_footer_item_visible(FooterItem::DynamicFiller)
            || controls.footer_item_cycle.is_some()
            || (controls.footer_track_position == FooterTrackPosition::Alongside
                && self.footer_track_rect(0).is_some())
            || self.touch_locked
            || self.ocr.is_some()
            || self.next_comic().is_some()
        {
            return None;
        }
        let progress = self.footer_progress()?;
        let items = self.custom_footer_parts(&progress, None);
        if items.is_empty() {
            return None;
        }

        let order = FooterItem::ordered(controls.footer_order);
        let before_filler = |item| {
            order
                .iter()
                .take_while(|&&candidate| candidate != FooterItem::DynamicFiller)
                .any(|&candidate| candidate == item)
        };
        let split = items.partition_point(|(item, _, _)| before_filler(*item));
        let font_size = self.footer_caption_font_px();
        let mut measure = |text: &str| {
            if let Some(file) = font_file {
                canvas.measure_with_font_file(text, font_size, controls.footer_item_bold, file)
            } else {
                canvas.measure_with_weight(text, font_size, controls.footer_item_bold)
            }
        };
        let left = Side::from_items(
            self,
            &items[..split],
            self.status_flow_label(),
            None,
            &mut measure,
        );
        let right = Side::from_items(
            self,
            &items[split..],
            None,
            self.read_alarm
                .footer()
                .map(|value| format!("Timer {value}")),
            &mut measure,
        );
        let space = measure(" ").max(1);
        (left.width + right.width + space <= width as i32).then_some(FillerLine {
            left,
            right,
            width,
        })
    }
}
