//! Layout for a book's bookmarks, highlights, and notes.
use std::borrow::Cow;

use primitive_ui::{BLACK, HAIRLINE, PAPER, RowAccessory, Rows, SECONDARY, TextStyle, chevron};

use super::list_density::{Geometry, fit_logical_font, list_text_pixels};
use super::{Action, Canvas, Item, Reader, Rect, Screen};

mod fit;
mod flexible;
use fit::TitleFit;
pub(super) use flexible::FlexibleMarks;

struct RowTypography {
    title_size: i32,
    title_height: i32,
    detail_size: i32,
    detail_height: i32,
    stacked: bool,
}

impl RowTypography {
    fn fit(logical_font: u8, row_height: i32) -> Self {
        let title_size = list_text_pixels(logical_font, row_height);
        let title_height = (title_size * 13 + 5) / 10;
        let stacked_limit = (row_height - title_height - 4) * 10 / 13;
        let stacked = stacked_limit >= 20;
        let mut detail_size = list_text_pixels(logical_font.saturating_sub(4), row_height).min(28);
        if stacked {
            detail_size = detail_size.min(stacked_limit);
        }
        Self {
            title_size,
            title_height,
            detail_size,
            detail_height: (detail_size * 13 + 5) / 10,
            stacked,
        }
    }
}

impl Screen {
    pub(super) fn is_bookmark_list(self) -> bool {
        matches!(
            self,
            Self::Bookmarks | Self::FixedBookmarks | Self::Highlights
        )
    }
}

impl Reader {
    pub(super) fn list_geometry(&self) -> Geometry {
        if self.screen.is_bookmark_list() {
            Geometry::dense(self.controls.bookmark_density)
        } else {
            Geometry::STANDARD
        }
    }

    pub(super) fn draw_saved_mark_rows(
        &self,
        canvas: &mut Canvas,
        group: &Rows<'_>,
        items: &[Item],
    ) {
        if group.rows.is_empty() {
            return;
        }
        canvas.card(group.bounds);
        let row_height = self.list_geometry().row_height;
        let logical_font = self.controls.bookmark_density.font();
        let mark_typography = RowTypography::fit(logical_font, row_height);
        let preferred_font = fit_logical_font(logical_font, row_height);
        // Search and import actions use the standard card text size (38px).
        let action_typography = RowTypography::fit(18, row_height);
        let pad = 32;
        let disclosure_size = 40;
        for (index, (item, action)) in group.rows.iter().zip(items).enumerate() {
            let Some(row) = group.row_rect(index) else {
                continue;
            };
            let is_mark = matches!(
                action.action,
                Action::Bookmark(_) | Action::Highlight(_) | Action::PdfHighlight(_)
            );
            let &RowTypography {
                title_size,
                title_height,
                detail_size,
                detail_height,
                stacked,
            } = if is_mark {
                &mark_typography
            } else {
                &action_typography
            };
            let pressed = canvas.control_pressed(row, false);
            if pressed {
                canvas.panel(row.inset(6), 16, 2, BLACK);
            }
            let ink = if pressed { PAPER } else { BLACK };
            let detail_ink = if pressed {
                PAPER - SECONDARY
            } else {
                SECONDARY
            };
            let link = item.accessory == RowAccessory::Disclosure;
            let text_x = row.x + pad;
            let text_width = row.w - 2 * pad - if link { 56 } else { 0 };
            let detail_width = text_width * 2 / 5;
            let caption_aside = Rect::new(
                text_x + text_width - detail_width,
                row.y + (row.h - detail_height) / 2,
                detail_width,
                detail_height,
            );
            let (mut title_bounds, mut detail_bounds) = match (item.detail, stacked) {
                (Some(_), true) => {
                    let top = row.y + (row.h - title_height - detail_height) / 2;
                    (
                        Rect::new(text_x, top, text_width, title_height),
                        Rect::new(text_x, top + title_height, text_width, detail_height),
                    )
                }
                (Some(_), false) => {
                    // Preserve dates and location captions beside one-line entries.
                    (
                        Rect::new(
                            text_x,
                            row.y + (row.h - title_height) / 2,
                            text_width - detail_width - 24,
                            title_height,
                        ),
                        caption_aside,
                    )
                }
                (None, _) => (
                    Rect::new(
                        text_x,
                        row.y + (row.h - title_height) / 2,
                        text_width,
                        title_height,
                    ),
                    Rect::new(0, 0, 0, 0),
                ),
            };
            if is_mark
                && self.controls.bookmark_shrink_to_fit
                && self.controls.bookmark_max_lines().is_none()
            {
                let title = if item.title.contains(['\n', '\r']) {
                    Cow::Owned(item.title.replace(['\n', '\r'], " "))
                } else {
                    Cow::Borrowed(item.title)
                };
                let mut reserve = if item.detail.is_some() && stacked {
                    detail_height
                } else {
                    0
                };
                let mut area = Rect::new(text_x, row.y, title_bounds.w, row.h - reserve - 6);
                let mut fit = TitleFit::measure(canvas, &title, area, preferred_font);
                if reserve > 0 && !fit.complete(&title) {
                    // Give long entries the full row height. The date and
                    // reading location keep their own width and type size.
                    reserve = 0;
                    detail_bounds = caption_aside;
                    area.w = text_width - detail_width - 24;
                    area.h = row.h - 6;
                    fit = TitleFit::measure(canvas, &title, area, preferred_font);
                }
                let fit = fit.shrink(canvas, &title, area);
                title_bounds = Rect::new(
                    text_x,
                    row.y + (row.h - fit.height() - reserve) / 2,
                    area.w,
                    fit.height(),
                );
                if reserve > 0 {
                    detail_bounds.y = title_bounds.bottom();
                }
                fit.draw(canvas, title_bounds, &title, ink);
            } else {
                canvas.text(
                    title_bounds,
                    item.title,
                    TextStyle::new(title_size)
                        .bold()
                        .break_long_words()
                        .ink(ink),
                );
            }
            if let Some(detail) = item.detail {
                canvas.text(
                    detail_bounds,
                    detail,
                    TextStyle::new(detail_size)
                        .break_long_words()
                        .ink(detail_ink),
                );
            }
            if link {
                chevron(
                    canvas,
                    Rect::new(
                        row.right() - pad - disclosure_size,
                        row.y + (row.h - disclosure_size) / 2,
                        disclosure_size,
                        disclosure_size,
                    ),
                    ink,
                );
            }
            if index + 1 < group.rows.len() {
                canvas.fill(
                    Rect::new(row.x + pad, row.bottom() - 2, row.w - 2 * pad, 2),
                    HAIRLINE,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
