//! The current book's font chooser and its source-font samples.
use super::{
    Action, BODY, BarPanel, CAPTION, Canvas, Item, Reader, Rect, Result, Rows, Screen, SystemTheme,
    TopGroup, WIDTH,
};
use primitive_app::with_storage;
use primitive_ui::{BLACK, GrayImage, HAIRLINE, PAPER};
use skrifa::{FontRef, MetadataProvider, string::StringId};
use std::fs;

const SPECIMEN_SIZE: (u32, u32) = (220, 68);
const SPECIMEN_TEXT: [&str; 9] = [
    "Aa 123",
    "أبجد",
    "日本語",
    "中文",
    "가나다",
    "Аа",
    "Αα",
    "अआइ",
    "אבג",
];

#[derive(Clone, Copy)]
pub(super) struct FontChooserReturn {
    screen: Screen,
    top: bool,
    bottom: bool,
    panel: Option<BarPanel>,
    group: Option<TopGroup>,
}

impl Reader {
    pub(super) fn open_font_chooser(&mut self) -> Result<()> {
        with_storage(|| fs::create_dir_all(&self.fonts_directory))?;
        self.available_fonts = crate::reading_fonts::available(&self.fonts_directory)?;
        let origin = self.font_chooser_return.unwrap_or(FontChooserReturn {
            screen: self.screen,
            top: self.reading_top,
            bottom: self.reading_bottom,
            panel: self.reading_panel,
            group: self.top_group,
        });
        self.show(Screen::ReadingFonts);
        self.font_chooser_return = Some(origin);
        let selected = self
            .selected_reading_font()
            .and_then(|selected| {
                self.available_fonts
                    .iter()
                    .position(|file| file == selected)
            })
            .map_or(0, |index| index + 1);
        self.focus_reading_list(selected);
        Ok(())
    }

    pub(super) fn close_font_chooser(&mut self) {
        if let Some(origin) = self.font_chooser_return.take() {
            self.show(origin.screen);
            self.reading_top = origin.top;
            self.reading_bottom = origin.bottom;
            self.reading_panel = origin.panel;
            self.top_group = origin.group;
        } else {
            self.show(Screen::Typography);
        }
    }

    fn selected_reading_font(&self) -> Option<&str> {
        self.library
            .state(self.open_book_id()?)?
            .preferences
            .font_file
            .as_deref()
    }

    pub(super) fn reading_font_rows(&self) -> Vec<Item> {
        let mut items = vec![Item::new("Device fonts", Action::SetFont(None))];
        items.extend(
            self.available_fonts
                .iter()
                .map(|filename| Item::new(filename, Action::SetFont(Some(filename.clone())))),
        );
        items.push(
            Item::new("Add more fonts", Action::Heading)
                .detail("Copy TTF or OTF files to reader/data/fonts"),
        );
        items
    }

    pub(super) fn draw_reading_font_rows(
        &self,
        canvas: &mut Canvas,
        group: &Rows<'_>,
        items: &[Item],
    ) {
        if items.is_empty() {
            return;
        }
        let appearance = self
            .system_theme
            .as_ref()
            .map(SystemTheme::dark)
            .or(self.controls.palette.inverted())
            .unwrap_or(false);
        let fonts: Vec<_> = self
            .fonts
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        let selected = self.selected_reading_font();
        canvas.card(group.bounds);
        for (index, item) in items.iter().enumerate() {
            let Some(rect) = group.row_rect(index) else {
                continue;
            };
            let pressed =
                !matches!(item.action, Action::Heading) && canvas.control_pressed(rect, false);
            if pressed {
                canvas.panel(rect.inset(6), 16, 2, BLACK);
            }
            let ink = if pressed { PAPER } else { BLACK };
            let detail_ink = if pressed { 170 } else { 85 };
            let mut detail = item.detail.clone();
            let preview = match &item.action {
                Action::SetFont(Some(filename)) => {
                    let preview = self.typeface_preview(filename, &fonts, pressed, appearance);
                    let description = if preview.is_some() {
                        filename.as_str()
                    } else {
                        "Preview unavailable"
                    };
                    detail = Some(if selected == Some(filename.as_str()) {
                        format!("Selected · {description}")
                    } else {
                        description.to_owned()
                    });
                    preview
                }
                Action::SetFont(None) if selected.is_none() => {
                    detail = Some("Selected".into());
                    None
                }
                _ => None,
            };
            let title = preview
                .as_ref()
                .map_or(item.title.as_str(), |(title, _)| title.as_str());
            let title_width = if matches!(item.action, Action::SetFont(Some(_))) {
                rect.w - 350
            } else {
                rect.w - 64
            };
            let title_y = rect.y + if detail.is_some() { 15 } else { 33 };
            canvas.text(
                Rect::new(rect.x + 32, title_y, title_width, 52),
                title,
                BODY.bold().ink(ink),
            );
            if let Some(detail) = detail {
                canvas.text(
                    Rect::new(rect.x + 32, title_y + 51, title_width, 38),
                    &detail,
                    CAPTION.ink(detail_ink),
                );
            }
            if let Some((_, specimen)) = preview {
                canvas.picture(
                    Rect::new(
                        rect.right() - specimen.width as i32 - 45,
                        rect.y + 24,
                        specimen.width as i32,
                        specimen.height as i32,
                    ),
                    specimen,
                );
            }
            if index + 1 < items.len() {
                canvas.fill(
                    Rect::new(rect.x + 32, rect.bottom() - 2, rect.w - 64, 2),
                    HAIRLINE,
                );
            }
        }
    }

    fn typeface_preview(
        &self,
        filename: &str,
        ui_fonts: &[String],
        pressed: bool,
        dark: bool,
    ) -> Option<(String, GrayImage)> {
        let path = crate::reading_fonts::resolve(&self.fonts_directory, filename).ok()?;
        let (title, specimen) = with_storage(|| {
            let data = fs::read(&path)?;
            let face = FontRef::new(&data).map_err(std::io::Error::other)?;
            let title = face
                .localized_strings(StringId::FULL_NAME)
                .english_or_first()
                .map(|name| {
                    name.to_string()
                        .chars()
                        .filter(|ch| !ch.is_control())
                        .take(128)
                        .collect::<String>()
                })
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| filename.to_owned());
            let chars = face.charmap();
            let specimen = SPECIMEN_TEXT
                .into_iter()
                .find(|text| text.chars().all(|ch| ch == ' ' || chars.map(ch).is_some()))
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    chars
                        .mappings()
                        .filter_map(|(codepoint, _)| char::from_u32(codepoint))
                        .filter(|ch| !ch.is_control() && !ch.is_whitespace())
                        .take(8)
                        .collect()
                });
            if specimen.is_empty() {
                return Err(std::io::Error::other("Font has no characters to preview"));
            }
            Ok((title, specimen))
        })
        .ok()?;
        // The shared canvas caches fonts it registers. A small per-file canvas
        // bounds the preview to one user font and leaves the reader canvas's cache untouched.
        let (width, height) = SPECIMEN_SIZE;
        let mut preview = Canvas::new_viewport(width, height, WIDTH, ui_fonts).ok()?;
        preview.begin();
        let rect = preview.bounds();
        if pressed {
            preview.fill(rect, BLACK);
        }
        with_storage(|| {
            preview.text_with_font_file(
                rect,
                &specimen,
                BODY.center().ink(if pressed { PAPER } else { BLACK }),
                path.to_str()
                    .ok_or_else(|| std::io::Error::other("Font path is unavailable"))?,
            );
            Ok(())
        })
        .ok()?;
        let pixels = preview.finish(dark);
        Some((
            title,
            GrayImage {
                width,
                height,
                pixels,
            },
        ))
    }
}

#[cfg(test)]
mod tests;
