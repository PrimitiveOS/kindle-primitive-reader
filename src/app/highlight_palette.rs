//! Shared named-color chooser for saved-mark filters; monochrome samples retain names.
use crate::highlight_style::HighlightColor;
use primitive_app::WIDTH;
use primitive_ui::{BODY, CAPTION, Canvas, Rect};

pub(super) const ALL: Rect = Rect::new(40, 158, WIDTH as i32 - 80, 104);
pub(super) fn cell(index: usize) -> Rect {
    cell_for(WIDTH as i32, index)
}

fn cell_for(screen_width: i32, index: usize) -> Rect {
    let width = (screen_width - 112) / 3;
    Rect::new(
        40 + index as i32 % 3 * (width + 16),
        294 + index as i32 / 3 * 148,
        width,
        124,
    )
}
pub(super) fn at(x: f32, y: f32) -> Option<Option<HighlightColor>> {
    at_width(WIDTH as i32, x, y)
}

pub(super) fn at_width(width: i32, x: f32, y: f32) -> Option<Option<HighlightColor>> {
    if Rect::new(ALL.x, ALL.y, width - 80, ALL.h).contains(x, y) {
        return Some(None);
    }
    HighlightColor::ALL
        .into_iter()
        .enumerate()
        .find_map(|(i, color)| cell_for(width, i).contains(x, y).then_some(Some(color)))
}
pub(super) fn draw(
    canvas: &mut Canvas,
    all: bool,
    selected: impl Fn(HighlightColor) -> bool,
    multiple: bool,
    language: super::interface_language::InterfaceLanguage,
) {
    let width = canvas.bounds().w;
    canvas.button(
        Rect::new(ALL.x, ALL.y, width - 80, ALL.h),
        language.bookmark_text("All colors"),
        all,
        false,
    );
    for (i, color) in HighlightColor::ALL.into_iter().enumerate() {
        let rect = cell_for(width, i);
        let chosen = selected(color) != canvas.control_pressed(rect, false);
        canvas.panel(rect, 16, 4, if chosen { 0 } else { 255 });
        let sample = Rect::new(rect.x + 20, rect.y + 38, 48, 48);
        draw_swatch(canvas, sample, color);
        canvas.text(
            Rect::new(sample.right() + 20, rect.y + 40, rect.w - 104, 64),
            language.bookmark_text(color.label()),
            BODY.bold().ink(if chosen { 255 } else { 0 }),
        );
    }
    canvas.text(Rect::new(40,776,width-80,130), language.bookmark_text(if multiple {
        "Choose one or more colors to export. Color and style filters are combined."
    } else {"Show saved highlights and notes with this color. Page bookmarks have no highlight color."}),CAPTION.lines(3));
}

pub(super) fn draw_swatch(canvas: &mut Canvas, sample: Rect, color: HighlightColor) {
    canvas.panel(sample, 4, 2, color.gray().unwrap_or(204));
}
