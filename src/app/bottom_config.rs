//! Reading adjustments that can be applied without leaving the visible page.
use super::{Action, BarPanel, Reader, Screen};
use crate::app::NumberField;
use crate::library::FixedTileAxis;
use crate::reading_controls::{ReadingOrientation, ScreenPalette};
use crate::{BaseStylesheet, FontHinting, Settings, TextAlignment};

pub(super) struct Choice {
    pub(super) title: String,
    pub(super) action: Action,
    pub(super) selected: bool,
    pub(super) live: bool,
}

impl Choice {
    fn live(title: impl Into<String>, action: Action) -> Self {
        Self {
            title: title.into(),
            action,
            selected: false,
            live: true,
        }
    }

    fn option(title: impl Into<String>, action: Action, selected: bool) -> Self {
        Self {
            title: title.into(),
            action,
            selected,
            live: true,
        }
    }

    fn link(title: &str, action: Action) -> Self {
        Self {
            title: title.into(),
            action,
            selected: false,
            live: false,
        }
    }
}

pub(super) struct ConfigRow {
    pub(super) label: String,
    pub(super) choices: Vec<Choice>,
}

impl ConfigRow {
    fn new(label: impl Into<String>, choices: impl IntoIterator<Item = Choice>) -> Self {
        Self {
            label: label.into(),
            choices: choices.into_iter().collect(),
        }
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
        if matches!(panel, BarPanel::TextScreen | BarPanel::ViewScreen) {
            let controls = self.controls;
            let mut rows = vec![ConfigRow::new(
                "Screen orientation",
                [
                    ReadingOrientation::FollowLauncher,
                    ReadingOrientation::Upright,
                    ReadingOrientation::UpsideDown,
                ]
                .map(|orientation| {
                    Choice::option(
                        orientation.label(),
                        Action::SetReadingControls(crate::reading_controls::ReadingControls {
                            orientation,
                            ..controls
                        }),
                        controls.orientation == orientation,
                    )
                }),
            )];
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
            return rows;
        }
        if let Some(active) = &self.active {
            let settings = active.reading.config.settings;
            let vertical = active.reading.probe.is_vertical();
            let text = |title, settings: Settings| Choice::live(title, Action::Text(settings));
            match panel {
                BarPanel::Text => {
                    let fine_step = settings.font_scale.physical_delta(0.5);
                    let smaller = settings.logical_font_size.map_or_else(
                        || {
                            Action::Text(Settings {
                                font_size: (settings.font_size - fine_step).max(20.),
                                ..settings
                            })
                        },
                        |n| Action::ApplyLogicalFont(active.id, (n - 0.5).max(12.)),
                    );
                    let larger = settings.logical_font_size.map_or_else(
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
                        fine.push(Choice::live(
                            "-1%",
                            Action::Text(settings.with_line_spacing_percent(spacing - 1)),
                        ));
                    }
                    fine.push(Choice::link(
                        "Set value",
                        Action::Show(Screen::NumberingValue(NumberField::LineSpacing(active.id))),
                    ));
                    if spacing < 200 {
                        fine.push(Choice::live(
                            "+1%",
                            Action::Text(settings.with_line_spacing_percent(spacing + 1)),
                        ));
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
                        edge_row("Left", settings.left_margin(), side_limit, |value| {
                            Settings {
                                margin_left: Some(value),
                                ..settings
                            }
                        }),
                        edge_row("Right", settings.right_margin(), side_limit, |value| {
                            Settings {
                                margin_right: Some(value),
                                ..settings
                            }
                        }),
                        edge_row("Top", settings.top_margin(), vertical_limit, |value| {
                            Settings {
                                margin_top: Some(value),
                                margin_bottom: if linked {
                                    Some(value)
                                } else {
                                    settings.margin_bottom
                                },
                                ..settings
                            }
                        }),
                        edge_row(
                            "Bottom",
                            settings.bottom_margin(),
                            vertical_limit,
                            |value| Settings {
                                margin_bottom: Some(value),
                                margin_top: if linked {
                                    Some(value)
                                } else {
                                    settings.margin_top
                                },
                                ..settings
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
                            "Base stylesheet",
                            BaseStylesheet::ALL
                                .into_iter()
                                .filter(|style| {
                                    *style != BaseStylesheet::FictionBook
                                        || matches!(
                                            active.entry.format,
                                            crate::library::DocumentFormat::Fb2
                                                | crate::library::DocumentFormat::Text
                                        )
                                })
                                .map(|base_stylesheet| {
                                    Choice::option(
                                        base_stylesheet.label(),
                                        Action::Text(Settings {
                                            base_stylesheet,
                                            ..settings
                                        }),
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
                            }),
                        ),
                    ];
                    rows.push(ConfigRow::new(
                        "Image scaling",
                        [("Fast", false), ("Smooth", true)].map(|(label, smooth_images)| {
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
                        if settings.hyphenation_mode() == crate::HyphenationMode::Automatic {
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
                        rows.push(ConfigRow::new(
                            "Fit",
                            [
                                Choice::option(
                                    "Fit page",
                                    Action::FitPage(false),
                                    prefs.zoom == 0
                                        && prefs.tile_zoom.is_none()
                                        && !prefs.width_fit
                                        && !prefs.height_fit,
                                ),
                                Choice::option("Fit width", Action::FitPage(true), prefs.width_fit),
                                Choice::option("Fit height", Action::FitHeight, prefs.height_fit),
                            ],
                        ));
                        rows.push(ConfigRow::new(
                            "Zoom",
                            ["100%", "125%", "150%", "200%", "300%"]
                                .into_iter()
                                .enumerate()
                                .map(|(index, title)| {
                                    Choice::option(
                                        title,
                                        Action::SetFixedZoom(index),
                                        prefs.zoom == index && prefs.tile_zoom.is_none(),
                                    )
                                }),
                        ));
                    }
                    if reading.supports_tile_zoom() {
                        for axis in [FixedTileAxis::Columns, FixedTileAxis::Rows] {
                            let selected = prefs
                                .tile_zoom
                                .filter(|tile| tile.axis == axis)
                                .map(|tile| tile.count_tenths);
                            let presets = (1_u8..=8).map(|count| {
                                let count_tenths = count * 10;
                                Choice::option(
                                    count.to_string(),
                                    Action::SetFixedTileZoom(axis, count_tenths),
                                    selected == Some(count_tenths),
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
                    if reading.supports_scan_contrast() {
                        rows.push(ConfigRow::new(
                            "Contrast",
                            [false, true].map(|higher| {
                                Choice::option(
                                    if higher {
                                        "Higher contrast"
                                    } else {
                                        "Normal contrast"
                                    },
                                    Action::ScanContrast(higher),
                                    reading.scan_contrast() == higher,
                                )
                            }),
                        ));
                        rows.push(ConfigRow::new(
                            "Scan white level",
                            [
                                ("Original", None),
                                ("Low", Some(248)),
                                ("Medium", Some(235)),
                                ("High", Some(220)),
                            ]
                            .map(|(title, level)| {
                                Choice::option(
                                    title,
                                    Action::ScanWhiteLevel(level),
                                    prefs.scan_white_level == level,
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
        Choice::option("Page turns", action(false), !scroll),
        Choice::option("Scroll", action(true), scroll),
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

fn edge_row(title: &str, current: u32, limit: u32, update: impl Fn(u32) -> Settings) -> ConfigRow {
    ConfigRow::new(
        format!("{title} · {current} px"),
        [-12, -1, 1, 12].map(|step: i32| {
            let value = (current as i32 + step).clamp(0, limit as i32) as u32;
            Choice::option(
                format!("{step:+}"),
                Action::Text(update(value)),
                value == current,
            )
        }),
    )
}
