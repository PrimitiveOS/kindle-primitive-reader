//! Reading adjustments that can be applied without leaving the visible page.
use crate::app::NumberField;
use crate::app::character_page_settings::MarginEdge;
use crate::library::FixedContentFit;
use super::{Action, BarPanel, Reader, Screen};
use crate::app::bar_icons::{
    BOTTOM_MARGIN_LESS, BOTTOM_MARGIN_MORE, FIT_FULL, FIT_HEIGHT, FIT_WIDTH, LEFT_MARGIN_LESS,
    LEFT_MARGIN_MORE, RIGHT_MARGIN_LESS, RIGHT_MARGIN_MORE, TOP_MARGIN_LESS, TOP_MARGIN_MORE,
    ZOOM_COLUMNS, ZOOM_CONTENT, ZOOM_IN, ZOOM_MANUAL, ZOOM_OUT, ZOOM_PAGE, ZOOM_ROWS,
};
use crate::app::reading_notifications::Source;
use crate::library::{FixedTileAxis, SCAN_GAMMA_DEFAULT, SCAN_GAMMA_MAX, SCAN_GAMMA_MIN};
use crate::reading_controls::{ReadingOrientation, ScreenPalette};
use crate::{BaseStylesheet, FontHinting, Settings, TextAlignment};
use primitive_ui::Icon;

fn format_scan_gamma(tenths: u16) -> String {
    format!("{}.{}", tenths / 10, tenths % 10)
}

fn zoom_factor_label(hundredths: u16) -> String {
    let whole = hundredths / 100;
    let fraction = hundredths % 100;
    let number = if fraction == 0 {
        whole.to_string()
    } else if fraction % 10 == 0 {
        format!("{whole}.{}", fraction / 10)
    } else {
        format!("{whole}.{fraction:02}")
    };
    format!("{number}×")
}

fn fixed_zoom_factor_choices(reading: &crate::fixed_session::FixedReading) -> Vec<Choice> {
    const PRESETS: [(u16, &str); 8] = [
        (70, "0.7"),
        (100, "1"),
        (150, "1.5"),
        (200, "2"),
        (300, "3"),
        (500, "5"),
        (1000, "10"),
        (2000, "20"),
    ];
    let current = reading.zoom_factor_hundredths();
    let selectable =
        reading.preferences().tile_zoom.is_none() && reading.preferences().precise_scale.is_none();
    let mut choices = PRESETS
        .into_iter()
        .map(|(factor, label)| {
            Choice::option(
                label,
                Action::SetFixedZoomFactor(factor),
                selectable && current == factor,
            )
        })
        .collect::<Vec<_>>();
    if selectable && !PRESETS.iter().any(|(factor, _)| *factor == current) {
        choices.push(Choice::option(
            format!("{:.2}", f32::from(current) / 100.0),
            Action::SetFixedZoomFactor(current),
            true,
        ));
    }
    choices
}

pub(super) struct Choice {
    pub(super) title: String,
    pub(super) action: Action,
    pub(super) selected: bool,
    pub(super) live: bool,
    pub(super) feedback: Option<(Source, String)>,
    pub(super) icon: Option<Icon>,
}

impl Choice {
    fn live(title: impl Into<String>, action: Action) -> Self {
        Self {
            title: title.into(),
            action,
            selected: false,
            live: true,
            feedback: None,
            icon: None,
        }
    }

    fn option(title: impl Into<String>, action: Action, selected: bool) -> Self {
        Self {
            title: title.into(),
            action,
            selected,
            live: true,
            feedback: None,
            icon: None,
        }
    }

    fn link(title: impl Into<String>, action: Action) -> Self {
        Self {
            title: title.into(),
            action,
            selected: false,
            live: false,
            feedback: None,
            icon: None,
        }
    }

    fn with_feedback(mut self, source: Source, message: impl Into<String>) -> Self {
        self.feedback = Some((source, message.into()));
        self
    }

    fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

pub(super) struct ConfigRow {
    pub(super) label: String,
    pub(super) choices: Vec<Choice>,
    pub(super) information: Option<&'static str>,
    pub(super) help: Option<Action>,
    pub(super) label_width: Option<i32>,
}

impl ConfigRow {
    fn unavailable(label: &str, message: &'static str) -> Self {
        let mut row = Self::new(label, []);
        row.information = Some(message);
        row
    }

    fn embedded_font_help(mut self) -> Self {
        self.help = Some(Action::EmbeddedFontReport);
        self.choices
            .push(Choice::link("Details", Action::EmbeddedFontReport));
        self
    }

    fn new(label: impl Into<String>, choices: impl IntoIterator<Item = Choice>) -> Self {
        Self {
            label: label.into(),
            choices: choices.into_iter().collect(),
            information: None,
            help: None,
            label_width: None,
        }
    }

    fn with_label_width(mut self, width: i32) -> Self {
        self.label_width = Some(width);
        self
    }
}

pub(super) const TEXT_SECTIONS: [(BarPanel, &str); 7] = [
    (BarPanel::Text, "Type"),
    (BarPanel::TextLayout, "Layout"),
    (BarPanel::TextMargins, "Margins"),
    (BarPanel::TextStyle, "Style"),
    (BarPanel::TextScreen, "Screen"),
    (BarPanel::TextRendering, "Rendering"),
    (BarPanel::TextSpacing, "Spacing"),
];

pub(super) const VIEW_SECTIONS: [(BarPanel, &str); 3] = [
    (BarPanel::View, "Page"),
    (BarPanel::ViewLayout, "Layout"),
    (BarPanel::ViewScreen, "Screen"),
];

pub(super) const SINGLE_VIEW_SECTIONS: [(BarPanel, &str); 2] =
    [(BarPanel::View, "Page"), (BarPanel::ViewScreen, "Screen")];

impl Reader {
    pub(super) fn bottom_config_rows(&self, panel: BarPanel) -> Vec<ConfigRow> {
        let mut rows = self.raw_bottom_config_rows(panel);
        if matches!(
            panel,
            BarPanel::Text
                | BarPanel::TextMargins
                | BarPanel::TextSpacing
                | BarPanel::TextRendering
                | BarPanel::View
                | BarPanel::Scan
        ) {
            // These audited panels contain fixed labels and numeric settings;
            // leave action/feedback payloads unchanged.
            let language = self.controls.interface_language;
            for row in &mut rows {
                row.label = language.other_bottom_title(&row.label);
                for choice in &mut row.choices {
                    choice.title = language.other_bottom_title(&choice.title);
                }
            }
        }
        let language = self.controls.interface_language;
        for row in &mut rows {
            if let Some(information) = row.information {
                row.information = Some(language.bottom_typography_text(information));
            }
        }
        rows
    }

    fn raw_bottom_config_rows(&self, panel: BarPanel) -> Vec<ConfigRow> {
        if matches!(panel, BarPanel::TextScreen | BarPanel::ViewScreen) {
            let controls = self.controls;
            let selected = self.displayed_reading_orientation();
            let mut rows = vec![ConfigRow::new(
                "Screen orientation",
                ReadingOrientation::ALL.map(|orientation| {
                    Choice::option(
                        orientation.label(),
                        Action::SetReadingOrientation(orientation),
                        selected == orientation,
                    )
                }),
            )];
            if self.landscape_choices_available() {
                rows.push(ConfigRow::new(
                    "Landscape",
                    [
                        ("Left", ReadingOrientation::LandscapeLeft),
                        ("Right", ReadingOrientation::LandscapeRight),
                    ]
                    .map(|(label, orientation)| {
                        Choice::option(
                            label,
                            Action::SetReadingOrientation(orientation),
                            selected == orientation,
                        )
                    }),
                ));
            }
            rows.push(ConfigRow::new(
                "Keep orientation",
                [("Off", false), ("On", true)].map(|(title, keep)| {
                    Choice::option(
                        title,
                        Action::KeepReadingOrientation(keep),
                        controls.sticky_orientation.is_some() == keep,
                    )
                }),
            ));
            // Under Primitive Launcher the theme is the launcher's: no light or dark switch here.
            if !self.under_launcher() {
                rows.extend([
                    ConfigRow::new(
                        "Interface palette",
                        [
                            ScreenPalette::FollowLauncher,
                            ScreenPalette::Light,
                            ScreenPalette::Dark,
                        ]
                        .map(|palette| {
                            Choice::option(
                                palette.label(),
                                Action::SetReadingControls(
                                    crate::reading_controls::ReadingControls {
                                        palette,
                                        ..controls
                                    },
                                ),
                                controls.palette == palette,
                            )
                        }),
                    ),
                    ConfigRow::new(
                        "Page colors",
                        [("Original", false), ("Inverted", true)].map(|(title, night_pages)| {
                            Choice::option(
                                title,
                                Action::SetReadingControls(
                                    crate::reading_controls::ReadingControls {
                                        night_pages,
                                        ..controls
                                    },
                                ),
                                controls.night_pages == night_pages,
                            )
                        }),
                    ),
                ]);
            }
            if panel == BarPanel::TextScreen
                && let Some(book) = &self.active
            {
                let displayed = self.top_header_for_book(book.id);
                let choices = [("Off", false), ("On", true)];
                rows.push(ConfigRow::new(
                    if book.reading.is_continuous() {
                        "Top status header (page turns)"
                    } else {
                        "Top status header"
                    },
                    choices.map(|(label, enabled)| {
                        Choice::option(
                            label,
                            Action::TopStatusHeader(enabled),
                            enabled == displayed,
                        )
                    }),
                ));
                rows.push(ConfigRow::new(
                    "Default for new books",
                    choices.map(|(label, enabled)| {
                        Choice::option(
                            label,
                            Action::ConfirmTopStatusHeaderDefault(enabled),
                            enabled == controls.top_header.default_enabled(),
                        )
                    }),
                ));
            }
            // These rows and choices are all Reader-owned fixed labels; never
            // apply this transformation to font names or publication content.
            let language = controls.interface_language;
            for row in &mut rows {
                row.label = language.text(&row.label).to_owned();
                for choice in &mut row.choices {
                    choice.title = language.text(&choice.title).to_owned();
                }
            }
            return rows;
        }
        if let Some(active) = &self.active {
            let settings = active.reading.config.settings;
            let vertical = active.reading.probe.is_vertical();
            let text = |title, settings: Settings| Choice::live(title, Action::Text(settings));
            match panel {
                BarPanel::Text => {
                    let fine_step = settings.font_scale.physical_delta(0.5);
                    let step_basis = self
                        .pending_logical_size(active.id)
                        .or(settings.logical_font_size);
                    let smaller = step_basis.map_or_else(
                        || {
                            Action::Text(Settings {
                                font_size: (settings.font_size - fine_step).max(20.),
                                ..settings
                            })
                        },
                        |n| Action::ApplyLogicalFont(active.id, (n - 0.5).max(12.)),
                    );
                    let larger = step_basis.map_or_else(
                        || {
                            Action::Text(Settings {
                                font_size: (settings.font_size + fine_step).min(100.),
                                ..settings
                            })
                        },
                        |n| Action::ApplyLogicalFont(active.id, (n + 0.5).min(255.)),
                    );
                    let mut rows = vec![ConfigRow::new(
                        format!("Size · {}", settings.font_label()),
                        [
                            Choice::live("Smaller text", smaller),
                            Choice::live("Larger text", larger),
                            Choice::link(
                                "More",
                                Action::Show(Screen::NumberingValue(NumberField::LogicalFont(
                                    active.id,
                                ))),
                            ),
                        ],
                    )];
                    for presets in crate::logical_font::PRESETS.chunks(6) {
                        rows.push(ConfigRow::new(
                            "Font size",
                            presets.iter().map(|&value| {
                                Choice::option(
                                    value.to_string(),
                                    Action::ApplyLogicalFont(active.id, f64::from(value)),
                                    settings.logical_font_size == Some(f64::from(value)),
                                )
                            }),
                        ));
                    }
                    if !vertical {
                        rows.push(ConfigRow::new(
                            "Reading mode",
                            mode_choices(active.reading.config.scroll, Action::ReadingMode),
                        ));
                    }
                    rows.push(ConfigRow::new(
                        "",
                        [
                            Choice::link("Reading font", Action::ShowFonts),
                            Choice::link("Text settings", Action::Show(Screen::Typography)),
                            Choice::link("Profiles", Action::Show(Screen::ReadingProfiles)),
                        ],
                    ));
                    return rows;
                }
                BarPanel::TextLayout => {
                    let mut rows = Vec::new();
                    if active.entry.format == crate::library::DocumentFormat::Epub {
                        if let Some(reason) = self.publisher_render_mode_unavailable() {
                            rows.push(ConfigRow::unavailable("Render mode", reason));
                        } else {
                            let current = self.book_publisher_render_mode();
                            rows.push(ConfigRow::new(
                                "Render mode",
                                crate::PublisherRenderMode::CHOICES.map(|mode| {
                                    Choice::option(
                                        mode.label(),
                                        Action::SetBookPublisherRenderMode(mode),
                                        mode == current,
                                    )
                                }),
                            ));
                            rows.push(ConfigRow::new(
                                "New-book mode",
                                [Choice::link(
                                    self.controls
                                        .publisher_render_mode_default
                                        .unwrap_or_default()
                                        .label(),
                                    Action::Show(Screen::PublisherRenderDefaults),
                                )],
                            ));
                        }
                    }
                    if !vertical && !active.reading.config.scroll {
                        let facing_pages = active.reading.config.facing_pages;
                        let title = if facing_pages && !active.reading.uses_facing_pages() {
                            "Pages · single view here"
                        } else {
                            "Pages"
                        };
                        rows.push(ConfigRow::new(
                            title,
                            [
                                Choice::option(
                                    "One page",
                                    Action::FacingPages(false),
                                    !facing_pages,
                                ),
                                Choice::option(
                                    "Two pages",
                                    Action::FacingPages(true),
                                    facing_pages,
                                ),
                            ],
                        ));
                    }
                    rows.push(margin_row(
                        "Side margins",
                        (settings.left_margin() + settings.right_margin()).div_ceil(2),
                        |value| Settings {
                            margin_sides: Some(value),
                            margin_left: None,
                            margin_right: None,
                            ..settings
                        },
                    ));
                    let spacing = settings.effective_line_spacing_percent();
                    let choices = |values: &[u16]| {
                        values
                            .iter()
                            .map(|&percent| {
                                Choice::option(
                                    format!("{percent}%"),
                                    Action::Text(settings.with_line_spacing_percent(percent)),
                                    spacing == percent,
                                )
                                .with_feedback(
                                    Source::Progress,
                                    format!("Line spacing · {percent}%"),
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    rows.push(ConfigRow::new(
                        "Line spacing",
                        choices(&crate::LINE_SPACING_PRESETS[..7]),
                    ));
                    rows.push(ConfigRow::new(
                        "",
                        choices(&crate::LINE_SPACING_PRESETS[7..]),
                    ));
                    let mut fine = Vec::with_capacity(3);
                    if spacing > 50 {
                        fine.push(
                            Choice::live(
                                "-1%",
                                Action::Text(settings.with_line_spacing_percent(spacing - 1)),
                            )
                            .with_feedback(
                                Source::Fine,
                                format!("Line spacing · {}%", spacing - 1),
                            ),
                        );
                    }
                    fine.push(Choice::link(
                        "Set value",
                        Action::Show(Screen::NumberingValue(NumberField::LineSpacing(active.id))),
                    ));
                    if spacing < 200 {
                        fine.push(
                            Choice::live(
                                "+1%",
                                Action::Text(settings.with_line_spacing_percent(spacing + 1)),
                            )
                            .with_feedback(
                                Source::Fine,
                                format!("Line spacing · {}%", spacing + 1),
                            ),
                        );
                    }
                    rows.push(ConfigRow::new(format!("Fine · {spacing}%"), fine));
                    if active.reading.config.scroll {
                        rows.push(ConfigRow::new(
                            "Page overlap",
                            [false, true].map(|enabled| {
                                Choice::option(
                                    if enabled { "On" } else { "Off" },
                                    Action::Text(Settings {
                                        page_overlap_enabled: enabled,
                                        ..settings
                                    }),
                                    settings.page_overlap_enabled == enabled,
                                )
                            }),
                        ));
                        if settings.page_overlap_enabled {
                            for (label, range) in [
                                ("Repeated lines · 1–5", 1..=5),
                                ("Repeated lines · 6–10", 6..=10),
                            ] {
                                rows.push(ConfigRow::new(
                                    label,
                                    range.map(|lines| {
                                        Choice::option(
                                            lines.to_string(),
                                            Action::Text(Settings {
                                                page_overlap_lines: lines,
                                                ..settings
                                            }),
                                            settings.page_overlap_lines == lines,
                                        )
                                    }),
                                ));
                            }
                            rows.push(ConfigRow::new(
                                "Overlap marker",
                                crate::PageOverlapStyle::ALL.map(|style| {
                                    Choice::option(
                                        style.label(),
                                        Action::Text(Settings {
                                            page_overlap_style: style,
                                            ..settings
                                        }),
                                        settings.page_overlap_style == style,
                                    )
                                }),
                            ));
                        }
                    }
                    // Only Reader-owned row and choice labels are translated;
                    // numeric values, action payloads and feedback stay intact.
                    let language = self.controls.interface_language;
                    for row in &mut rows {
                        row.label = language.text_layout_title(&row.label);
                        for choice in &mut row.choices {
                            choice.title = language.text_layout_title(&choice.title);
                        }
                    }
                    return rows;
                }
                BarPanel::TextMargins => {
                    let linked = settings.link_vertical_margins;
                    let side_limit = settings.width.min(settings.height) / 4;
                    let vertical_limit = settings.height / 4;
                    let middle = (settings.top_margin() + settings.bottom_margin()).div_ceil(2);
                    let independent = Settings {
                        link_vertical_margins: false,
                        ..settings
                    };
                    let sync = Settings {
                        margin_top: Some(middle),
                        margin_bottom: Some(middle),
                        link_vertical_margins: true,
                        ..settings
                    };
                    return vec![
                        margin_preset_row(
                            "L/R preset",
                            settings,
                            side_limit,
                            |value| {
                                settings.left_margin() == value
                                    && settings.right_margin() == value
                            },
                            |value| Settings {
                                margin_left: Some(value),
                                margin_right: Some(value),
                                ..settings
                            },
                        )
                        .with_label_width(MARGIN_PRESET_LABEL_WIDTH),
                        horizontal_margin_row(settings),
                        margin_preset_row(
                            "Top presets",
                            settings,
                            vertical_limit,
                            |value| settings.top_margin() == value,
                            |value| Settings {
                                margin_top: Some(value),
                                margin_bottom: if linked {
                                    Some(value)
                                } else {
                                    settings.margin_bottom
                                },
                                ..settings
                            },
                        )
                        .with_label_width(MARGIN_PRESET_LABEL_WIDTH),
                        edge_row(
                            "Top",
                            settings.top_margin(),
                            vertical_limit,
                            MarginEdge::Top,
                            |value| Settings {
                                margin_top: Some(value),
                                margin_bottom: if linked {
                                    Some(value)
                                } else {
                                    settings.margin_bottom
                                },
                                ..settings
                            },
                        ),
                        margin_preset_row(
                            "Bottom presets",
                            settings,
                            vertical_limit,
                            |value| settings.bottom_margin() == value,
                            |value| Settings {
                                margin_bottom: Some(value),
                                margin_top: if linked {
                                    Some(value)
                                } else {
                                    settings.margin_top
                                },
                                ..settings
                            },
                        )
                        .with_label_width(MARGIN_PRESET_LABEL_WIDTH),
                        edge_row(
                            "Bottom",
                            settings.bottom_margin(),
                            vertical_limit,
                            MarginEdge::Bottom,
                            |value| {
                                Settings {
                                    margin_bottom: Some(value),
                                    margin_top: if linked {
                                        Some(value)
                                    } else {
                                        settings.margin_top
                                    },
                                    ..settings
                                }
                            },
                        ),
                        ConfigRow::new(
                            "Top and bottom",
                            [
                                Choice::option("Independent", Action::Text(independent), !linked),
                                Choice::option("Linked", Action::Text(sync), linked),
                            ],
                        ),
                        ConfigRow::new(
                            "",
                            [text(
                                "Reset margins",
                                Settings {
                                    margin: Settings::default().margin,
                                    margin_sides: None,
                                    margin_left: None,
                                    margin_right: None,
                                    margin_top: None,
                                    margin_bottom: None,
                                    link_vertical_margins: false,
                                    ..settings
                                },
                            )],
                        ),
                    ];
                }
                BarPanel::TextSpacing if !vertical => {
                    return vec![
                        ConfigRow::new(
                            "Word spacing",
                            [
                                ("Small", crate::WordSpacePercent::SMALL),
                                ("Medium", crate::WordSpacePercent::MEDIUM),
                                ("Large", crate::WordSpacePercent::LARGE),
                            ]
                            .map(|(label, value)| {
                                Choice::option(
                                    label,
                                    Action::ApplyWordSpacing(active.id, value),
                                    settings.percent_word_spacing == Some(value),
                                )
                            }),
                        ),
                        ConfigRow::new(
                            match settings.percent_word_spacing {
                                Some(pair) => format!("{}% / {}%", pair.scaling, pair.minimum),
                                None => format!("Legacy: {:+} px", settings.word_spacing),
                            },
                            [Choice::link(
                                "More",
                                Action::Show(Screen::NumberingValue(NumberField::WordSpacing(
                                    self.word_spacing_draft(active.id),
                                ))),
                            )],
                        ),
                        ConfigRow::new(
                            "Letter spacing",
                            [
                                ("Tight", -0.75),
                                ("Normal", 0.0),
                                ("Wide", 1.0),
                                ("Wider", 2.0),
                            ]
                            .map(|(title, letter_spacing)| {
                                Choice::option(
                                    title,
                                    Action::Text(Settings {
                                        letter_spacing,
                                        ..settings
                                    }),
                                    settings.letter_spacing == letter_spacing,
                                )
                            }),
                        ),
                        ConfigRow::new(
                            "Word expansion",
                            [(0, "None"), (5, "Some"), (15, "More")].map(
                                |(word_expansion, label)| {
                                    Choice::option(
                                        label,
                                        Action::Text(Settings {
                                            word_expansion,
                                            ..settings
                                        }),
                                        word_expansion == settings.word_expansion,
                                    )
                                },
                            ),
                        ),
                        ConfigRow::new(
                            format!("Fine tune · {}%", settings.word_expansion),
                            [
                                text(
                                    "-1%",
                                    Settings {
                                        word_expansion: settings.word_expansion.saturating_sub(1),
                                        ..settings
                                    },
                                ),
                                text(
                                    "+1%",
                                    Settings {
                                        word_expansion: settings
                                            .word_expansion
                                            .saturating_add(1)
                                            .min(20),
                                        ..settings
                                    },
                                ),
                            ],
                        ),
                        ConfigRow::new(
                            format!("CJK width · {}%", settings.cjk_width_percent),
                            [
                                ("-5%", -5),
                                ("-1%", -1),
                                ("100%", 0),
                                ("+1%", 1),
                                ("+5%", 5),
                            ]
                            .map(|(label, delta)| {
                                let value = if delta == 0 {
                                    100
                                } else {
                                    (i16::from(settings.cjk_width_percent) + delta).clamp(100, 150)
                                        as u8
                                };
                                Choice::option(
                                    label,
                                    Action::Text(Settings {
                                        cjk_width_percent: value,
                                        ..settings
                                    }),
                                    value == settings.cjk_width_percent,
                                )
                            }),
                        ),
                    ];
                }
                BarPanel::TextRendering => {
                    let mut rows = vec![
                        ConfigRow::new(
                            "Style tweaks",
                            [Choice::link("Open", Action::Show(Screen::StyleTweaks))],
                        ),
                        ConfigRow::new(
                            "Base stylesheet (hold for default)",
                            BaseStylesheet::ALL
                                .into_iter()
                                .filter(|style| {
                                    *style != BaseStylesheet::FictionBook
                                        || matches!(
                                            active.reading_format(),
                                            crate::library::DocumentFormat::Fb2
                                                | crate::library::DocumentFormat::Fb3
                                                | crate::library::DocumentFormat::Text
                                        )
                                })
                                .map(|base_stylesheet| {
                                    Choice::option(
                                        if base_stylesheet == BaseStylesheet::FictionBook
                                            && active.reading_format()
                                                == crate::library::DocumentFormat::Fb3
                                        {
                                            "FB3"
                                        } else {
                                            base_stylesheet.label()
                                        },
                                        Action::SetBaseStylesheet(base_stylesheet),
                                        settings.base_stylesheet == base_stylesheet,
                                    )
                                }),
                        ),
                        ConfigRow::new(
                            "Publisher CSS",
                            [("Off", false), ("On", true)].map(|(title, publisher_styles)| {
                                Choice::option(
                                    title,
                                    Action::Text(Settings {
                                        publisher_styles,
                                        ..settings
                                    }),
                                    settings.publisher_styles == publisher_styles,
                                )
                            }),
                        ),
                        (if !settings.publisher_styles {
                            ConfigRow::unavailable("Embedded fonts", "Turn on Publisher CSS")
                        } else if !active.has_embedded_fonts() {
                            ConfigRow::unavailable("Embedded fonts", "No embedded fonts found")
                        } else {
                            ConfigRow::new(
                                "Embedded fonts",
                                [("Off", false), ("On", true)].map(|(title, embedded_fonts)| {
                                    Choice::option(
                                        title,
                                        Action::Text(Settings {
                                            embedded_fonts,
                                            ..settings
                                        }),
                                        settings.embedded_fonts == embedded_fonts,
                                    )
                                }),
                            )
                        })
                        .embedded_font_help(),
                        ConfigRow::new(
                            "Zoom (dpi)",
                            crate::RENDER_DPI_PRESETS.map(|render_dpi| {
                                Choice::option(
                                    if render_dpi == 0 {
                                        "Off".to_string()
                                    } else {
                                        render_dpi.to_string()
                                    },
                                    Action::Text(Settings {
                                        render_dpi,
                                        ..settings
                                    }),
                                    settings.render_dpi == render_dpi,
                                )
                            }),
                        ),
                        ConfigRow::new(
                            "Custom zoom",
                            [
                                Choice::option(
                                    if settings.render_dpi == 0 {
                                        "Off".to_string()
                                    } else {
                                        format!("{} dpi", settings.render_dpi)
                                    },
                                    Action::Heading,
                                    !crate::RENDER_DPI_PRESETS.contains(&settings.render_dpi),
                                ),
                                Choice::link(
                                    "Set value",
                                    Action::Show(Screen::NumberingValue(NumberField::ContentDpi(
                                        active.id,
                                    ))),
                                ),
                            ],
                        ),
                        ConfigRow::new(
                            "Font kerning",
                            [
                                ("Off", crate::FontKerningMode::Off),
                                ("Fast", crate::FontKerningMode::Fast),
                                ("Good", crate::FontKerningMode::Good),
                                ("Best", crate::FontKerningMode::Best),
                            ]
                            .map(|(title, font_kerning_mode)| {
                                Choice::option(
                                    title,
                                    Action::Text(Settings {
                                        font_kerning: font_kerning_mode
                                            != crate::FontKerningMode::Off,
                                        font_kerning_mode: Some(font_kerning_mode),
                                        ..settings
                                    }),
                                    settings.effective_font_kerning_mode() == font_kerning_mode,
                                )
                                .with_feedback(Source::Toggles, format!("Font kerning · {title}"))
                            }),
                        ),
                        ConfigRow::new(
                            "Font hinting",
                            [
                                ("Off", FontHinting::Off),
                                ("Native", FontHinting::Native),
                                ("Auto", FontHinting::Auto),
                            ]
                            .map(|(title, font_hinting)| {
                                Choice::option(
                                    title,
                                    Action::Text(Settings {
                                        font_hinting,
                                        ..settings
                                    }),
                                    settings.font_hinting == font_hinting,
                                )
                                .with_feedback(Source::Toggles, format!("Font hinting · {title}"))
                            }),
                        ),
                    ];
                    rows.push(ConfigRow::new(
                        "Image scaling",
                        [("Fast", false), ("Best", true)].map(|(label, smooth_images)| {
                            Choice::option(
                                label,
                                Action::Text(Settings {
                                    smooth_images,
                                    ..settings
                                }),
                                smooth_images == settings.smooth_images,
                            )
                        }),
                    ));
                    if self.uses_night_pages() {
                        rows.push(ConfigRow::new(
                            "Night images",
                            [("Original", false), ("Inverted", true)].map(
                                |(label, invert_night_images)| {
                                    Choice::option(
                                        label,
                                        Action::Text(Settings {
                                            invert_night_images,
                                            ..settings
                                        }),
                                        invert_night_images == settings.invert_night_images,
                                    )
                                },
                            ),
                        ));
                    }
                    return rows;
                }
                BarPanel::TextStyle => {
                    let mut rows = vec![ConfigRow::new(
                        "Font weight",
                        [-1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 3.0].map(|delta| {
                            Choice::option(
                                format!("{delta:+.2}"),
                                Action::Text(Settings {
                                    font_weight_delta: delta,
                                    bold_text: false,
                                    ..settings
                                }),
                                settings.font_weight_delta == delta && !settings.bold_text,
                            )
                        }),
                    )];
                    rows.push(ConfigRow::new(
                        format!("Fine tune · {:+.2}", settings.font_weight_delta),
                        [
                            ("-1", -1.0),
                            ("-0.25", -0.25),
                            ("0", 0.0),
                            ("+0.25", 0.25),
                            ("+1", 1.0),
                        ]
                        .map(|(label, step)| {
                            text(
                                label,
                                Settings {
                                    font_weight_delta: if step == 0.0 {
                                        0.0
                                    } else {
                                        (settings.font_weight_delta + step).clamp(-3.0, 5.5)
                                    },
                                    bold_text: false,
                                    ..settings
                                },
                            )
                        }),
                    ));
                    rows.push(ConfigRow::new(
                        "Font contrast",
                        crate::font_render::GAMMA_PRESETS.map(|index| {
                            Choice::option(
                                crate::font_render::gamma_label(index),
                                Action::Text(Settings {
                                    font_gamma_index: index,
                                    text_contrast: false,
                                    ..settings
                                }),
                                settings.font_gamma_index == index && !settings.text_contrast,
                            )
                        }),
                    ));
                    rows.push(ConfigRow::new(
                        format!(
                            "Fine tune · {}",
                            crate::font_render::gamma_label(settings.font_gamma_index)
                        ),
                        [("-", -1), ("1.00", 0), ("+", 1)].map(|(label, step)| {
                            let index = if step == 0 {
                                crate::font_render::NORMAL_GAMMA
                            } else {
                                (i16::from(settings.font_gamma_index) + step).clamp(0, 56) as u8
                            };
                            text(
                                label,
                                Settings {
                                    font_gamma_index: index,
                                    text_contrast: false,
                                    ..settings
                                },
                            )
                        }),
                    ));
                    if !vertical {
                        rows.push(ConfigRow::new(
                            "Alignment",
                            [
                                ("Publisher", TextAlignment::Publisher),
                                ("Start", TextAlignment::Start),
                                ("Center", TextAlignment::Center),
                                ("Justify", TextAlignment::Justify),
                            ]
                            .map(|(title, alignment)| {
                                Choice::option(
                                    title,
                                    Action::Text(Settings {
                                        alignment,
                                        ..settings
                                    }),
                                    settings.alignment == alignment,
                                )
                            }),
                        ));
                        rows.push(ConfigRow::new(
                            "Hyphenation",
                            crate::HyphenationMode::ALL.map(|mode| {
                                Choice::option(
                                    mode.label(),
                                    Action::Text(settings.with_hyphenation(mode)),
                                    settings.hyphenation_mode() == mode,
                                )
                            }),
                        ));
                        if settings.hyphenation_mode() != crate::HyphenationMode::Off {
                            rows.push(ConfigRow::new(
                                "Custom rules",
                                [false, true].map(|enabled| {
                                    Choice::option(
                                        if enabled { "On" } else { "Off" },
                                        Action::SetUserHyphenation(enabled),
                                        self.user_hyphenation.enabled == enabled,
                                    )
                                }),
                            ));
                        }
                    }
                    rows.push(ConfigRow::new(
                        "",
                        [
                            text(
                                if settings.override_fonts {
                                    "Use publisher fonts"
                                } else {
                                    "Use selected font"
                                },
                                Settings {
                                    override_fonts: !settings.override_fonts,
                                    ..settings
                                },
                            ),
                            Choice::live("Reset text settings", Action::ResetText),
                        ],
                    ));
                    // This panel contains built-in labels and numeric values,
                    // never a publication title or the selected font name.
                    let language = self.controls.interface_language;
                    for row in &mut rows {
                        row.label = language.text_style_title(&row.label);
                        for choice in &mut row.choices {
                            choice.title = language.text_style_title(&choice.title);
                        }
                    }
                    return rows;
                }
                _ => {}
            }
        }
        if let Some(fixed) = &self.fixed {
            let reading = &fixed.reading;
            let prefs = reading.preferences();
            match panel {
                BarPanel::View => {
                    let mut rows = Vec::new();
                    if !reading.is_spread() {
                        let fit_axis = if prefs.zoom == 0
                            && prefs.tile_zoom.is_none()
                            && prefs.precise_scale.is_none()
                            && !prefs.manual_zoom
                        {
                            Some(prefs.zoom_fit_axis.or(prefs.content_fit).unwrap_or(
                                if prefs.width_fit {
                                    FixedContentFit::Width
                                } else if prefs.height_fit {
                                    FixedContentFit::Height
                                } else {
                                    FixedContentFit::Full
                                },
                            ))
                        } else {
                            None
                        };
                        rows.push(ConfigRow::new(
                            "Zoom to",
                            [
                                Choice::option(
                                    "Page",
                                    Action::ZoomTargetContent(false),
                                    prefs.tile_zoom.is_none()
                                        && !prefs.manual_zoom
                                        && prefs.zoom == 0
                                        && prefs.precise_scale.is_none()
                                        && prefs.content_fit.is_none(),
                                )
                                .with_icon(ZOOM_PAGE),
                                Choice::option(
                                    "Content",
                                    Action::ZoomTargetContent(true),
                                    prefs.tile_zoom.is_none()
                                        && !prefs.manual_zoom
                                        && prefs.zoom == 0
                                        && prefs.precise_scale.is_none()
                                        && prefs.content_fit.is_some(),
                                )
                                .with_icon(ZOOM_CONTENT),
                                Choice::option(
                                    "Columns",
                                    Action::SetFixedTileZoom(
                                        FixedTileAxis::Columns,
                                        reading.tile_zoom_count_tenths(FixedTileAxis::Columns),
                                    ),
                                    prefs
                                        .tile_zoom
                                        .is_some_and(|tile| tile.axis == FixedTileAxis::Columns),
                                )
                                .with_icon(ZOOM_COLUMNS),
                                Choice::option(
                                    "Rows",
                                    Action::SetFixedTileZoom(
                                        FixedTileAxis::Rows,
                                        reading.tile_zoom_count_tenths(FixedTileAxis::Rows),
                                    ),
                                    prefs
                                        .tile_zoom
                                        .is_some_and(|tile| tile.axis == FixedTileAxis::Rows),
                                )
                                .with_icon(ZOOM_ROWS),
                                Choice::option(
                                    "Manual",
                                    Action::ZoomTargetManual,
                                    prefs.manual_zoom,
                                )
                                .with_icon(ZOOM_MANUAL),
                            ],
                        ));
                        if !prefs.manual_zoom && prefs.tile_zoom.is_none() {
                            rows.push(ConfigRow::new(
                                "Fit",
                                [
                                    ("Full", FixedContentFit::Full),
                                    ("Width", FixedContentFit::Width),
                                    ("Height", FixedContentFit::Height),
                                ]
                                .into_iter()
                                .map(|(label, fit)| {
                                    Choice::option(
                                        label,
                                        Action::SetFixedFitAxis(fit),
                                        fit_axis == Some(fit),
                                    )
                                    .with_icon(match fit {
                                        FixedContentFit::Full => FIT_FULL,
                                        FixedContentFit::Width => FIT_WIDTH,
                                        FixedContentFit::Height => FIT_HEIGHT,
                                    })
                                }),
                            ));
                        } else if prefs.manual_zoom {
                            rows.push(ConfigRow::new(
                                "Zoom factor",
                                fixed_zoom_factor_choices(reading),
                            ));
                            let zoom_factor = reading.zoom_factor_hundredths();
                            rows.push(ConfigRow::new(
                                "Fine",
                                [
                                    Choice::option(
                                        "−0.1×",
                                        Action::AdjustFixedZoomFactor(-10),
                                        false,
                                    )
                                    .with_icon(ZOOM_OUT),
                                    Choice::option(
                                        zoom_factor_label(zoom_factor),
                                        Action::SetFixedZoomFactor(zoom_factor),
                                        true,
                                    ),
                                    Choice::option(
                                        "+0.1×",
                                        Action::AdjustFixedZoomFactor(10),
                                        false,
                                    )
                                    .with_icon(ZOOM_IN),
                                ],
                            ));
                        } else if let Some(tile) = prefs.tile_zoom {
                            let axis = tile.axis;
                            let selected = reading.tile_zoom_count_tenths(axis);
                            let presets = (1_u8..=8).map(|count| {
                                let count_tenths = count * 10;
                                Choice::option(
                                    count.to_string(),
                                    Action::SetFixedTileZoom(axis, count_tenths),
                                    selected == count_tenths,
                                )
                            });
                            let custom = Choice::link(
                                "Custom",
                                Action::Show(Screen::NumberingValue(NumberField::FixedTileZoom(
                                    axis,
                                ))),
                            );
                            rows.push(ConfigRow::new(axis.label(), presets.chain([custom])));
                        }
                    }
                    rows.push(ConfigRow::new(
                        "Rotation",
                        ["0°", "90°", "180°", "270°"].into_iter().enumerate().map(
                            |(index, title)| {
                                Choice::option(
                                    title,
                                    Action::SetFixedRotation(index as u8),
                                    prefs.turns == index as u8,
                                )
                            },
                        ),
                    ));
                    return rows;
                }
                BarPanel::ViewLayout => {
                    let mut rows = Vec::new();
                    if reading.supports_continuous() {
                        rows.push(ConfigRow::new(
                            "Reading mode",
                            mode_choices(reading.is_continuous(), Action::FixedScroll),
                        ));
                        rows.push(ConfigRow::new(
                            "Page gap",
                            [0, 12, 24, 48].map(|gap| {
                                Choice::option(
                                    format!("{gap} px"),
                                    Action::FixedGap(gap),
                                    prefs.continuous_gap == gap,
                                )
                            }),
                        ));
                    }
                    if reading.supports_spreads() {
                        rows.push(ConfigRow::new(
                            "Page layout",
                            [false, true].map(|spread| {
                                Choice::option(
                                    if spread { "Two pages" } else { "One page" },
                                    Action::ComicSpread(spread),
                                    reading.is_spread() == spread,
                                )
                            }),
                        ));
                    }
                    if reading.allows_page_overlap() {
                        let enabled = prefs.page_overlap_enabled;
                        let style = prefs.page_overlap_style.unwrap_or_default();
                        rows.push(ConfigRow::new(
                            "Page overlap",
                            [("Off", false), ("On", true)].map(|(label, active)| {
                                Choice::option(
                                    label,
                                    Action::SetFixedPageOverlap(active, style),
                                    active == enabled,
                                )
                            }),
                        ));
                        if enabled {
                            rows.push(ConfigRow::new(
                                "Overlap marker",
                                crate::PageOverlapStyle::ALL.map(|option| {
                                    Choice::option(
                                        option.label(),
                                        Action::SetFixedPageOverlap(true, option),
                                        option == style,
                                    )
                                }),
                            ));
                        }
                    }
                    if reading.supports_rtl() {
                        let ltr = Choice::option(
                            "Left to right",
                            Action::ReversePageDirection,
                            !prefs.right_to_left,
                        );
                        let rtl = Choice::option(
                            "Right to left",
                            Action::ReversePageDirection,
                            prefs.right_to_left,
                        );
                        rows.push(ConfigRow::new("Reading direction", [ltr, rtl]));
                    }
                    if self.fixed_profiles_available() {
                        rows.push(ConfigRow::new(
                            "Reading profiles",
                            [Choice::link(
                                "Profiles",
                                Action::Show(Screen::ReadingProfiles),
                            )],
                        ));
                    }
                    // These labels are built in; the pixel-gap choices and
                    // all action values are preserved exactly.
                    let language = self.controls.interface_language;
                    for row in &mut rows {
                        row.label = language.view_layout_title(&row.label).to_owned();
                        for choice in &mut row.choices {
                            choice.title = language.view_layout_title(&choice.title).to_owned();
                        }
                    }
                    return rows;
                }
                BarPanel::Scan => {
                    let mut rows = Vec::new();
                    rows.push(ConfigRow::new(
                        "Image dithering",
                        [("Off", false), ("On", true)].map(|(title, enabled)| {
                            Choice::option(
                                title,
                                Action::FixedDither(enabled),
                                prefs.dither == enabled,
                            )
                        }),
                    ));
                    if reading.supports_scan_gamma() {
                        let gamma = prefs.scan_gamma_tenths.unwrap_or(SCAN_GAMMA_DEFAULT);
                        rows.push(ConfigRow::new(
                            "Contrast",
                            [8, 10, 15, 20, 40, 60, 100, 500].map(|tenths| {
                                Choice::option(
                                    format_scan_gamma(tenths),
                                    Action::ScanGamma(tenths),
                                    !prefs.scan_contrast && gamma == tenths,
                                )
                                .with_feedback(
                                    Source::Progress,
                                    format!("Contrast · {}", format_scan_gamma(tenths)),
                                )
                            }),
                        ));
                        let steps = [("-1.0", -10), ("-0.1", -1), ("+0.1", 1), ("+1.0", 10)]
                            .into_iter()
                            .filter_map(|(label, delta)| {
                                let value = gamma
                                    .saturating_add_signed(delta)
                                    .clamp(SCAN_GAMMA_MIN, SCAN_GAMMA_MAX);
                                (value != gamma).then(|| {
                                    Choice::live(label, Action::ScanGamma(value)).with_feedback(
                                        Source::Fine,
                                        format!("Contrast · {}", format_scan_gamma(value)),
                                    )
                                })
                            });
                        let adaptive = Choice::option(
                            "Adaptive",
                            Action::ScanContrast(true),
                            prefs.scan_contrast,
                        );
                        let label = if prefs.scan_contrast {
                            "Fine contrast".into()
                        } else {
                            format!("Fine · {}", format_scan_gamma(gamma))
                        };
                        rows.push(ConfigRow::new(label, steps.chain([adaptive])));
                    } else if reading.supports_scan_contrast() {
                        rows.push(ConfigRow::new(
                            "Contrast",
                            [("Normal contrast", false), ("Higher contrast", true)].map(
                                |(title, higher)| {
                                    Choice::option(
                                        title,
                                        Action::ScanContrast(higher),
                                        prefs.scan_contrast == higher,
                                    )
                                },
                            ),
                        ));
                    }
                    if reading.supports_scan_contrast() {
                        let current = prefs.scan_white_level.unwrap_or(255);
                        let presets = [255_u8, 224, 192, 160, 128, 96, 64, 32].map(|level| {
                            Choice::option(
                                level.to_string(),
                                Action::ScanWhiteLevel((level != 255).then_some(level)),
                                current == level,
                            )
                        });
                        rows.push(ConfigRow::new(
                            "Scan white level",
                            presets.into_iter().chain([Choice::link(
                                "Fine tune",
                                Action::Show(Screen::NumberingValue(NumberField::ScanWhiteLevel)),
                            )]),
                        ));
                    }
                    if reading.supports_dewatermark() && prefs.scan_white_level.is_none() {
                        rows.push(ConfigRow::new(
                            "Dewatermark",
                            [
                                ("Off", false),
                                ("On", true),
                            ]
                            .map(|(label, enabled)| {
                                Choice::option(
                                    label,
                                    Action::Dewatermark(enabled),
                                    prefs.dewatermark == enabled,
                                )
                            }),
                        ));
                    }
                    if reading.supports_text() {
                        rows.push(ConfigRow::new(
                            "Auto straighten",
                            [0, 5, 10, 15, 25].map(|limit| {
                                let label = if limit == 0 {
                                    "Off".to_owned()
                                } else {
                                    format!("{limit}°")
                                };
                                Choice::option(
                                    label,
                                    Action::StraightenScan(limit),
                                    prefs.straighten_limit == limit,
                                )
                            }),
                        ));
                        if self.ocr_assets.is_some() || prefs.force_ocr {
                            rows.push(ConfigRow::new(
                                "Text selection",
                                [("Embedded first", false), ("Force OCR", true)].map(
                                    |(label, enabled)| {
                                        Choice::option(
                                            label,
                                            Action::ForceOcr(enabled),
                                            prefs.force_ocr == enabled,
                                        )
                                    },
                                ),
                            ));
                        }
                        let limit = prefs.straighten_limit;
                        let mut fine = Vec::with_capacity(2);
                        if limit > 0 {
                            fine.push(Choice::live(
                                "Decrease 1°",
                                Action::StraightenScan(limit - 1),
                            ));
                        }
                        if limit < crate::library::MAX_STRAIGHTEN_DEGREES {
                            fine.push(Choice::live(
                                "Increase 1°",
                                Action::StraightenScan(limit + 1),
                            ));
                        }
                        rows.push(ConfigRow::new(format!("Limit · {limit}°"), fine));
                    }
                    if reading.crop_margin_applies() {
                        let current = prefs.crop_margin();
                        let presets = [5, 10, 25, 40, 55, 70, 85, 100].map(|margin| {
                            Choice::option(
                                format!("{:.2}", f32::from(margin) / 100.0),
                                Action::SetCropMargin(margin),
                                current == margin,
                            )
                        });
                        let custom = Choice::link(
                            "Custom",
                            Action::Show(Screen::NumberingValue(NumberField::CropMargin)),
                        );
                        rows.push(ConfigRow::new(
                            format!("Margin · {:.2} in", f32::from(current) / 100.0),
                            presets.into_iter().chain([custom]),
                        ));
                    }
                    let tools = self
                        .scan_items()
                        .into_iter()
                        .filter_map(|item| {
                            if matches!(item.action, Action::ScanContrast(_)) {
                                return None;
                            }
                            Some(Choice::link(&item.title, item.action))
                        })
                        .collect::<Vec<_>>();
                    if !tools.is_empty() {
                        rows.push(ConfigRow::new("", tools));
                    }
                    return rows;
                }
                _ => {}
            }
        }
        Vec::new()
    }

    pub(super) fn supports_word_spacing(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| !active.reading.probe.is_vertical())
    }

    pub(super) fn supports_scan_panel(&self) -> bool {
        self.fixed.as_ref().is_some_and(|fixed| {
            let reading = &fixed.reading;
            reading.supports_crop() || reading.supports_text() || reading.supports_scan_contrast()
        })
    }

    pub(super) fn supports_view_layout(&self) -> bool {
        self.fixed.as_ref().is_some_and(|fixed| {
            let reading = &fixed.reading;
            reading.supports_continuous() || reading.supports_spreads() || reading.supports_rtl()
        })
    }
}

fn mode_choices(scroll: bool, action: impl Fn(bool) -> Action) -> [Choice; 2] {
    [
        Choice::option("Page turns", action(false), !scroll)
            .with_feedback(Source::Toggles, "Reading mode · Page turns"),
        Choice::option("Scroll", action(true), scroll)
            .with_feedback(Source::Toggles, "Reading mode · Scroll"),
    ]
}

fn margin_row(title: &str, current: u32, update: impl Fn(u32) -> Settings) -> ConfigRow {
    ConfigRow::new(
        format!("{title} · {current} px"),
        [
            Choice::live("Less", Action::Text(update(current.saturating_sub(12)))),
            Choice::live("More", Action::Text(update((current + 12).min(150)))),
        ],
    )
}

fn horizontal_margin_row(settings: Settings) -> ConfigRow {
    ConfigRow::new(
        "L/R margins",
        [
            Choice::link(
                format!(
                    "{} / {} px",
                    settings.left_margin(),
                    settings.right_margin()
                ),
                Action::Show(Screen::NumberingValue(NumberField::ReadingSideMargins)),
            ),
            Choice::live("L−", Action::AdjustReadingMargin(MarginEdge::Left, -1))
                .with_icon(LEFT_MARGIN_LESS),
            Choice::live("L+", Action::AdjustReadingMargin(MarginEdge::Left, 1))
                .with_icon(LEFT_MARGIN_MORE),
            Choice::live("R−", Action::AdjustReadingMargin(MarginEdge::Right, -1))
                .with_icon(RIGHT_MARGIN_LESS),
            Choice::live("R+", Action::AdjustReadingMargin(MarginEdge::Right, 1))
                .with_icon(RIGHT_MARGIN_MORE),
        ],
    )
}

fn edge_row(
    title: &str,
    current: u32,
    limit: u32,
    edge: MarginEdge,
    update: impl Fn(u32) -> Settings,
) -> ConfigRow {
    let mut choices = Vec::new();
    choices.extend([-12, -1, 1, 12].map(|step: i32| {
        let value = (current as i32 + step).clamp(0, limit as i32) as u32;
        let action = if step == -1 || step == 1 {
            Action::AdjustReadingMargin(edge, step as i16)
        } else {
            Action::Text(update(value))
        };
        let icon = match (edge, step.is_negative()) {
            (MarginEdge::Top, true) => TOP_MARGIN_LESS,
            (MarginEdge::Top, false) => TOP_MARGIN_MORE,
            (MarginEdge::Bottom, true) => BOTTOM_MARGIN_LESS,
            (MarginEdge::Bottom, false) => BOTTOM_MARGIN_MORE,
            _ => unreachable!("only vertical margins have edge controls"),
        };
        Choice::option(format!("{step:+}"), action, value == current).with_icon(icon)
    }));
    choices.push(Choice::link(
        "Set",
        Action::Show(Screen::NumberingValue(NumberField::ReadingMargin(edge))),
    ));
    ConfigRow::new(format!("{title} · {current} px"), choices)
}

/// KOReader's page-margin choices are expressed in size-scaled logical pixels.
/// Keep those named stops alongside Reader's physical-pixel fine adjustments.
const KOREADER_MARGIN_STOPS: [(u32, &str); 9] = [
    (5, "5"),
    (10, "10"),
    (15, "15"),
    (20, "20"),
    (30, "30"),
    (50, "50"),
    (70, "70"),
    (100, "100"),
    (140, "140"),
];

// Full-width preset hit areas stay at least 100 px wide on a 1,072 px display.
const MARGIN_PRESET_LABEL_WIDTH: i32 = 140;

fn scaled_koreader_margin(value: u32, settings: Settings) -> u32 {
    let short_side = settings.width.min(settings.height);
    value.saturating_mul(short_side).div_ceil(600)
}

fn margin_preset_row(
    title: &str,
    settings: Settings,
    limit: u32,
    selected: impl Fn(u32) -> bool,
    update: impl Fn(u32) -> Settings,
) -> ConfigRow {
    ConfigRow::new(
        title,
        KOREADER_MARGIN_STOPS.map(|(value, label)| {
            let pixels = scaled_koreader_margin(value, settings).min(limit);
            Choice::option(label, Action::Text(update(pixels)), selected(pixels))
        }),
    )
}
