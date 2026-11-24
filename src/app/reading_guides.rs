//! Optional edge guides for paced reading practice.
use super::{Action, Item, NumberField, Reader, Result, Screen};
use primitive_ui::GrayImage;
use serde::{Deserialize, Serialize};
use std::ops::Range;

const SHIFT: u16 = 30;
const SHIFT_LIMIT: u16 = 370;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(super) enabled: bool,
    thickness: u8,
    margin: u16,
    intensity: u8,
    interval: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            thickness: 2,
            margin: 100,
            intensity: 3,
            interval: 100,
        }
    }
}

impl Settings {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn valid(self) -> bool {
        (1..=20).contains(&self.thickness)
            && (10..=450).contains(&self.margin)
            && (1..=10).contains(&self.intensity)
            && self.interval <= 9_999
    }

    pub(super) fn status(self) -> &'static str {
        if self.enabled { "On" } else { "Off" }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Field {
    Thickness,
    Margin,
    Intensity,
    Interval,
}

impl Field {
    const ALL: [Self; 4] = [
        Self::Thickness,
        Self::Margin,
        Self::Intensity,
        Self::Interval,
    ];

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Thickness => "Line thickness",
            Self::Margin => "Margin from the edges",
            Self::Intensity => "Line intensity",
            Self::Interval => "Move guides inward",
        }
    }

    pub(super) fn hint(self) -> &'static str {
        match self {
            Self::Thickness => "Enter 1 to 20 pixels",
            Self::Margin => "Enter 1.0 to 45.0 percent",
            Self::Intensity => "Enter 1 to 10, from light to dark",
            Self::Interval => "Enter 1 to 9,999 pages, or 0 to keep the guides still",
        }
    }

    pub(super) fn value(self, settings: Settings) -> String {
        match self {
            Self::Thickness => settings.thickness.to_string(),
            Self::Margin => format!("{}.{:01}", settings.margin / 10, settings.margin % 10),
            Self::Intensity => settings.intensity.to_string(),
            Self::Interval => settings.interval.to_string(),
        }
    }

    fn detail(self, settings: Settings) -> String {
        let value = self.value(settings);
        match self {
            Self::Thickness => format!("{value} px"),
            Self::Margin => format!("{value}%"),
            Self::Intensity => format!("{value} of 10"),
            Self::Interval if settings.interval == 0 => "Off".into(),
            Self::Interval => format!("{value} pages"),
        }
    }

    fn parse(self, value: &str, mut settings: Settings) -> Option<Settings> {
        let value = value.trim();
        match self {
            Self::Thickness => settings.thickness = value.parse().ok()?,
            Self::Margin => {
                let (whole, fraction) = value.split_once('.').unwrap_or((value, "0"));
                let [digit @ b'0'..=b'9'] = fraction.as_bytes() else {
                    return None;
                };
                let whole = whole.parse::<u16>().ok()?;
                settings.margin = whole
                    .checked_mul(10)?
                    .checked_add(u16::from(digit - b'0'))?;
            }
            Self::Intensity => settings.intensity = value.parse().ok()?,
            Self::Interval => settings.interval = value.parse().ok()?,
        }
        settings.valid().then_some(settings)
    }
}

#[derive(Default)]
pub(super) struct Progress {
    count: u16,
    extra: u16,
}

impl Progress {
    pub(super) fn settings_changed(&mut self, current: Settings, next: Settings) {
        if current.margin != next.margin {
            self.extra = 0;
        }
    }

    pub(super) fn update(&mut self, settings: Settings) {
        if !settings.enabled {
            return;
        }
        if settings.interval != 0
            && self.count >= settings.interval
            && settings.margin + self.extra < SHIFT_LIMIT
        {
            self.extra += SHIFT;
            self.count = 0;
        } else {
            self.count = self.count.saturating_add(1);
        }
    }

    pub(super) fn guides(&self, settings: Settings, width: u32, height: u32) -> Option<Lines> {
        if !settings.enabled || width == 0 || height == 0 {
            return None;
        }
        let margin = u32::from(settings.margin + self.extra);
        let left = width * margin / 1_000;
        let thickness = u32::from(settings.thickness);
        // Stock paints the second child after advancing by the first line's width.
        let right = width * (1_000 - margin) / 1_000 + thickness;
        let span = |x: u32| x..(x + thickness).min(width);
        let top = height * 5 / 100;
        let ink = 255 - (255 * u16::from(settings.intensity) / 10) as u8;
        Some(Lines {
            left: span(left),
            right: span(right),
            rows: top..top + height * 9 / 10,
            ink,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};
    use primitive_app::{App, Change, HEIGHT, WIDTH};
    use primitive_ui::{Canvas, Input};

    #[test]
    fn reading_guides_follow_the_stock_geometry_gray_and_progression() {
        let settings = Settings {
            enabled: true,
            interval: 2,
            ..Settings::default()
        };
        let mut progress = Progress::default();
        let projection = |progress: &Progress| progress.guides(settings, 1264, 1680).unwrap();
        let initial = projection(&progress);
        assert_eq!(
            (initial.left, initial.right, initial.rows, initial.ink),
            (126..128, 1139..1141, 84..1596, 179)
        );
        progress.update(settings);
        progress.update(settings);
        assert_eq!(projection(&progress).left, 126..128);
        progress.update(settings);
        let shifted = projection(&progress);
        assert_eq!((shifted.left, shifted.right), (164..166, 1101..1103));
        let disabled = Settings {
            enabled: false,
            ..settings
        };
        progress.settings_changed(settings, disabled);
        progress.update(disabled);
        assert!(progress.guides(disabled, 1264, 1680).is_none());
        progress.settings_changed(disabled, settings);
        assert_eq!(projection(&progress).left, 164..166);
        for _ in 0..300 {
            progress.update(settings);
        }
        assert_eq!(projection(&progress).left, 467..469);

        let still = Settings {
            interval: 0,
            ..settings
        };
        let mut progress = Progress::default();
        for _ in 0..110 {
            progress.update(still);
        }
        assert_eq!(progress.guides(still, 1264, 1680).unwrap().left, 126..128);
        assert!(progress.guides(Settings::default(), 1264, 1680).is_none());
        assert_eq!(Field::Margin.parse("17.5", still).unwrap().margin, 175);
        for value in ["0.0", "45.1", "25.02", "NaN", "-5.0", "9999"] {
            assert!(Field::Margin.parse(value, still).is_none(), "{value}");
        }
    }

    #[test]
    fn reading_guides_draw_in_the_document_and_reopen_without_moving_the_book() {
        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.system_theme = None;
        app.open_path(&fixture.book).unwrap();
        app.follow(crate::epub::Destination {
            path: "EPUB/chapters/dusk.xhtml".into(),
            fragment: Some("dusk".into()),
        })
        .unwrap();
        let original_bytes = std::fs::read(&fixture.book).unwrap();
        let position = app.current_position();
        let font_paths = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let original = primitive_app::frame(&app, &mut canvas, false);

        let entry = app
            .top_config_items(crate::app::top_config::TopGroup::Appearance)
            .into_iter()
            .find(|item| item.title == "Reading guides")
            .unwrap();
        app.activate(entry.action).unwrap();
        assert_eq!(app.screen, Screen::ReadingGuides);
        let geometry = app.list_geometry();
        let first = geometry.row(0);
        assert_eq!(
            app.input(Input::Tap((first.x + 20) as f32, (first.y + 20) as f32)),
            Change::Partial
        );
        assert!(app.controls.reading_guides.enabled);
        app.set_reading_guide(Field::Intensity, "5").unwrap();
        assert!(app.set_reading_guide(Field::Intensity, "0").is_err());
        app.set_reading_guide(Field::Interval, "0").unwrap();
        capture_test_screen(&app, &fonts, "READER_READING_GUIDES_SETTINGS", false);
        app.show(Screen::Book);
        let guided = primitive_app::frame(&app, &mut canvas, false);
        let at = |x, y| (y * WIDTH + x) as usize;
        for x in [126, 127, 1139, 1140] {
            assert_eq!(guided[at(x, 84)], 128);
            assert_eq!(guided[at(x, 83)], original[at(x, 83)]);
        }
        assert_eq!(
            &guided[(crate::session::PAGE_HEIGHT * WIDTH) as usize..],
            &original[(crate::session::PAGE_HEIGHT * WIDTH) as usize..]
        );
        capture_test_screen(&app, &fonts, "READER_READING_GUIDES_PAGE", false);
        app.save().unwrap();

        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.system_theme = None;
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(reopened.current_position(), position);
        assert_eq!(
            reopened.controls.reading_guides,
            app.controls.reading_guides
        );
        assert_eq!(std::fs::read(&fixture.book).unwrap(), original_bytes);
    }

    #[test]
    fn reading_guides_are_visible_in_fixed_pages_and_pdf_reflow() {
        let fixture = Fixture::new();
        let source = fixture.root.join("Afternoon.pdf");
        std::fs::copy(crate::test_fixture("scan-reflow/vector.pdf"), &source).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.system_theme = None;
        app.open_path(&source).unwrap();
        app.controls.reading_guides.enabled = true;
        let fonts_as_strings = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts_as_strings).unwrap();
        let guide_pixel = (84 * WIDTH + 126) as usize;
        assert_eq!(
            primitive_app::frame(&app, &mut canvas, false)[guide_pixel],
            179
        );
        app.read_fixed_text(false).unwrap();
        assert_eq!(app.screen, Screen::ReadFixedText);
        assert_eq!(
            primitive_app::frame(&app, &mut canvas, false)[guide_pixel],
            179
        );
        capture_test_screen(&app, &fonts, "READER_READING_GUIDES_PDF", false);
    }

    #[test]
    fn reading_guides_follow_the_accepted_scan_viewport() {
        use crate::app::selection_viewport_tests::{adopt, capture};
        use crate::session::ReadingViewport;

        let fixture = Fixture::new();
        let source = fixture.root.join("Harbor survey.pdf");
        std::fs::copy(crate::test_fixture("scan-reflow/raster.pdf"), &source).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&source).unwrap();
        app.jump_fixed(1, None).unwrap();
        let ocr = crate::ocr::parse_tsv(
            include_bytes!("../../tests/fixtures/scan-reflow/page-2-ocr.tsv"),
            1350,
            1800,
        )
        .unwrap();
        app.fixed
            .as_mut()
            .unwrap()
            .reading
            .remember_ocr(1, "eng", ocr.text)
            .unwrap();
        app.read_fixed_text(true).unwrap();

        for (generation, size, lanes, start) in [
            (1, ReadingViewport::PORTRAIT, [126, 1139], 84),
            (2, ReadingViewport::LANDSCAPE, [168, 1514], 63),
        ] {
            let mut canvas = adopt(&mut app, size, generation);
            app.controls.reading_guides.enabled = false;
            let plain = primitive_app::frame(&app, &mut canvas, false);
            app.controls.reading_guides.enabled = true;
            let marked = primitive_app::frame(&app, &mut canvas, false);
            let width = size.dimensions().0 as usize;
            for lane in lanes {
                let before_line = (start - 1) * width + lane;
                let on_line = start * width + lane;
                assert_eq!(marked[before_line], plain[before_line]);
                assert_eq!(&marked[on_line..on_line + 2], &[179, 179]);
            }
            let footer = size.page_height() as usize * width;
            assert_eq!(marked[footer..], plain[footer..]);
            assert_ne!(marked, plain);
            if size == ReadingViewport::LANDSCAPE {
                assert_eq!(marked[84 * width + 126], plain[84 * width + 126]);
                capture(
                    &app,
                    &mut canvas,
                    "landscape-scan-reading-guides.png",
                    false,
                );
            }
        }
    }
}

pub(crate) struct Lines {
    left: Range<u32>,
    right: Range<u32>,
    rows: Range<u32>,
    ink: u8,
}

impl Lines {
    pub(crate) fn paint(self, page: &mut GrayImage, visible: &Range<u32>) {
        let first = self.rows.start.max(visible.start);
        let last = self.rows.end.min(visible.end);
        if first >= last || page.width == 0 {
            return;
        }
        let width = page.width as usize;
        for row in page
            .pixels
            .chunks_exact_mut(width)
            .skip(first as usize)
            .take((last - first) as usize)
        {
            for span in [&self.left, &self.right] {
                let end = span.end.min(page.width);
                if span.start < end {
                    row[span.start as usize..end as usize].fill(self.ink);
                }
            }
        }
    }
}

impl Reader {
    pub(super) fn guide_lines(&self) -> Option<Lines> {
        let (width, height) = self.reading_viewport().dimensions();
        self.reading_guide_progress
            .guides(self.controls.reading_guides, width, height)
    }

    pub(super) fn reading_guide_entry(&self) -> Item {
        Item::new("Reading guides", Action::Show(Screen::ReadingGuides))
            .detail(self.controls.reading_guides.status())
    }

    pub(super) fn reading_guide_items(&self) -> Vec<Item> {
        let current = self.controls.reading_guides;
        let mut controls = self.controls;
        controls.reading_guides.enabled = !current.enabled;
        let mut items = vec![Item::new(
            "Show reading guides",
            Action::SetReadingControls(controls),
        )];
        items.extend(Field::ALL.map(|field| {
            Item::new(
                field.title(),
                Action::Show(Screen::NumberingValue(NumberField::ReadingGuide(field))),
            )
            .detail(field.detail(current))
        }));
        items.push(Item::new(
            "About reading guides",
            Action::Show(Screen::ReadingGuidesAbout),
        ));
        items
    }

    pub(super) fn set_reading_guide(&mut self, field: Field, value: &str) -> Result<()> {
        let mut controls = self.controls;
        controls.reading_guides = field
            .parse(value, controls.reading_guides)
            .ok_or_else(|| field.hint())?;
        self.activate(Action::SetReadingControls(controls))?;
        self.show(Screen::ReadingGuides);
        Ok(())
    }
}
