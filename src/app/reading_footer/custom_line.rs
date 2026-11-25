//! Fit and compose custom footer items with the active status font.
use std::borrow::Cow;

use icu_segmenter::GraphemeClusterSegmenter;

use super::{
    FOOTER_ICON_GAP, FOOTER_ICON_SIZE, FooterItem, FooterSymbolStyle, FooterTextItem,
    FooterTrackPosition, Reader, StatusBar, footer_item_icon,
};
use primitive_ui::{Canvas, Icon, Rect, TextStyle};

fn fit_item(value: &str, max_width: i32, mut measure: impl FnMut(&str) -> i32) -> Cow<'_, str> {
    if measure(value) <= max_width {
        return Cow::Borrowed(value);
    }
    if measure("…") > max_width {
        return Cow::Borrowed("");
    }

    let boundaries: Vec<_> = GraphemeClusterSegmenter::new().segment_str(value).collect();
    let mut best = String::from("…");
    let mut start = 1;
    let mut end = boundaries.len() - 1;
    while start < end {
        let index = (start + end) / 2;
        let fitted = format!("{}…", value[..boundaries[index]].trim_end());
        if measure(&fitted) <= max_width {
            best = fitted;
            start = index + 1;
        } else {
            end = index;
        }
    }
    Cow::Owned(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_item_uses_a_complete_grapheme_and_accounts_for_ellipsis() {
        let measure = |text: &str| text.chars().count() as i32 * 10;
        assert_eq!(fit_item("e\u{301}xyz", 30, measure), "e\u{301}…");
        assert_eq!(fit_item("e\u{301}xyz", 20, measure), "…");
        assert_eq!(fit_item("e\u{301}xyz", 5, measure), "");
        assert_eq!(fit_item("Already fits", 120, measure), "Already fits");
    }
}

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
        let viewport_width = reader.reading_viewport().dimensions().0 as i32;
        let mut side = Self::default();
        let mut has_item = before.is_some();
        let mut text = before.unwrap_or_default();
        let mut merged = false;
        for (item, value, merge) in items {
            if has_item && !merged && !merge {
                text.push_str(separator);
            }
            let prefix = symbols.prefix_for(*item, inverted);
            let icon = (symbols == FooterSymbolStyle::Icons)
                .then(|| footer_item_icon(*item, inverted))
                .flatten();
            if let Some(icon) = icon {
                side.push_text(&mut text, measure);
                side.push(Fragment::Icon(icon));
            }
            text.push_str(prefix);
            if let Some(item) = FooterTextItem::for_item(*item) {
                let width =
                    viewport_width * i32::from(reader.controls.footer_text_widths.get(item)) / 100;
                let icon_width = icon.map_or(0, |_| FOOTER_ICON_SIZE + FOOTER_ICON_GAP);
                let available = width - icon_width - measure(prefix);
                let value = value.replace(['\n', '\r', '\u{2028}', '\u{2029}'], " ");
                text.push_str(&fit_item(&value, available, &mut *measure));
            } else {
                text.push_str(value);
            }
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
            let available = rect.right() - x;
            if available <= 0 {
                break;
            }
            match fragment {
                Fragment::Icon(icon) => {
                    if available < FOOTER_ICON_SIZE {
                        break;
                    }
                    let top = rect.y + (rect.h - FOOTER_ICON_SIZE) / 2;
                    canvas.icon(
                        Rect::new(x, top, FOOTER_ICON_SIZE, FOOTER_ICON_SIZE),
                        *icon,
                        0,
                    );
                }
                Fragment::Text { text, width } if *width > 0 => {
                    let text_rect = Rect::new(x, rect.y, (*width).min(available), rect.h);
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

pub(in crate::app) struct CustomLine {
    left: Side,
    spaced: Option<(Side, u32)>,
}

impl CustomLine {
    pub(in crate::app) fn width(&self) -> i32 {
        self.spaced
            .as_ref()
            .map_or(self.left.width, |(_, width)| *width as i32)
    }

    pub(in crate::app) fn gap_width(&self) -> Option<u32> {
        self.spaced.as_ref().map(|(_, width)| *width)
    }

    pub(in crate::app) fn draw(
        &self,
        canvas: &mut Canvas,
        rect: Rect,
        style: TextStyle,
        font_file: Option<&str>,
    ) {
        self.left.draw(canvas, rect.x, rect, style, font_file);
        if let Some((right, _)) = &self.spaced {
            right.draw(canvas, rect.right() - right.width, rect, style, font_file);
        }
    }
}

impl Reader {
    pub(in crate::app) fn custom_footer_line(
        &self,
        canvas: &mut Canvas,
        width: u32,
        font_file: Option<&str>,
    ) -> Option<CustomLine> {
        let controls = self.controls;
        if controls.status_bar != StatusBar::Custom {
            return None;
        }
        let progress = self.footer_progress()?;
        let cycle = controls.footer_item_cycle;
        let items = self.custom_footer_parts(&progress, cycle, cycle.is_none());
        if items.is_empty() {
            return None;
        }

        let font_size = self.footer_caption_font_px();
        let mut measure = |text: &str| {
            if let Some(file) = font_file {
                canvas.measure_with_font_file(text, font_size, controls.footer_item_bold, file)
            } else {
                canvas.measure_with_weight(text, font_size, controls.footer_item_bold)
            }
        };
        let before = self.status_flow_label();
        let after = self
            .read_alarm
            .footer()
            .map(|value| format!("Timer {value}"));
        if self.custom_footer_item_visible(FooterItem::DynamicFiller)
            && cycle.is_none()
            && !(controls.footer_track_position == FooterTrackPosition::Alongside
                && self.footer_track_rect(0).is_some())
            && !self.touch_locked
            && self.ocr.is_none()
            && self.next_comic().is_none()
        {
            let order = FooterItem::ordered(controls.footer_order);
            let before_filler = |item| {
                order
                    .iter()
                    .take_while(|&&candidate| candidate != FooterItem::DynamicFiller)
                    .any(|&candidate| candidate == item)
            };
            let split = items.partition_point(|(item, _, _)| before_filler(*item));
            let left = Side::from_items(self, &items[..split], before.clone(), None, &mut measure);
            let right = Side::from_items(self, &items[split..], None, after.clone(), &mut measure);
            let space = measure(" ").max(1);
            if left.width + right.width + space <= width as i32 {
                return Some(CustomLine {
                    left,
                    spaced: Some((right, width)),
                });
            }
        }
        Some(CustomLine {
            left: Side::from_items(self, &items, before, after, &mut measure),
            spaced: None,
        })
    }
}
