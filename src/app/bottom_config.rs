//! Reading adjustments that can be applied without leaving the visible page.
use super::{Action, BarPanel, Reader, Screen};
use crate::reading_controls::{ReadingOrientation, ScreenPalette};
use crate::{FontHinting, Settings, TextAlignment};

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

pub(super) const TEXT_SECTIONS: [(BarPanel, &str); 6] = [
    (BarPanel::Text, "Type"),
    (BarPanel::TextLayout, "Layout"),
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
            return vec![
                ConfigRow::new(
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
                ),
                ConfigRow::new(
                    "Screen theme",
                    [
                        ScreenPalette::FollowLauncher,
                        ScreenPalette::Light,
                        ScreenPalette::Dark,
                    ]
                    .map(|palette| {
                        Choice::option(
                            palette.label(),
                            Action::SetReadingControls(crate::reading_controls::ReadingControls {
                                palette,
                                ..controls
                            }),
                            controls.palette == palette,
                        )
                    }),
                ),
            ];
        }
        if let Some(active) = &self.active {
            let settings = active.reading.config.settings;
            let vertical = active.reading.probe.is_vertical();
            let text = |title, settings: Settings| Choice::live(title, Action::Text(settings));
            match panel {
                BarPanel::Text => {
                    let mut rows = vec![ConfigRow::new(
                        format!("Size · {:.0} px", settings.font_size),
                        [
                            text(
                                "Smaller text",
                                Settings {
                                    font_size: (settings.font_size - 4.0).max(20.0),
                                    ..settings
                                },
                            ),
                            text(
                                "Larger text",
                                Settings {
                                    font_size: (settings.font_size + 4.0).min(100.0),
                                    ..settings
                                },
                            ),
                        ],
                    )];
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
                    rows.extend([
                        margin_row("Side margins", settings.side_margin(), |value| Settings {
                            margin_sides: Some(value),
                            ..settings
                        }),
                        margin_row("Top margin", settings.top_margin(), |value| Settings {
                            margin_top: Some(value),
                            ..settings
                        }),
                        margin_row("Bottom margin", settings.bottom_margin(), |value| {
                            Settings {
                                margin_bottom: Some(value),
                                ..settings
                            }
                        }),
                        ConfigRow::new(
                            format!("Lines · {:.2}×", settings.line_height),
                            [
                                text(
                                    "Tighter",
                                    Settings {
                                        line_height: (settings.line_height - 0.1).max(1.0),
                                        ..settings
                                    },
                                ),
                                text(
                                    "Looser",
                                    Settings {
                                        line_height: (settings.line_height + 0.1).min(2.2),
                                        ..settings
                                    },
                                ),
                            ],
                        ),
                    ]);
                    return rows;
                }
                BarPanel::TextSpacing if !vertical => {
                    return vec![
                        ConfigRow::new(
                            "Word spacing",
                            [
                                ("Tight", -2.0),
                                ("Normal", 0.0),
                                ("Wide", 4.0),
                                ("Wider", 8.0),
                            ]
                            .map(|(title, word_spacing)| {
                                Choice::option(
                                    title,
                                    Action::Text(Settings {
                                        word_spacing,
                                        ..settings
                                    }),
                                    settings.word_spacing == word_spacing,
                                )
                            }),
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
                    ];
                }
                BarPanel::TextRendering => {
                    return vec![
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
                            "Font kerning",
                            [("Off", false), ("On", true)].map(|(title, font_kerning)| {
                                Choice::option(
                                    title,
                                    Action::Text(Settings {
                                        font_kerning,
                                        ..settings
                                    }),
                                    settings.font_kerning == font_kerning,
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
                }
                BarPanel::TextStyle => {
                    let mut rows = vec![ConfigRow::new(
                        "Text weight",
                        [false, true].map(|bold_text| {
                            Choice::option(
                                if bold_text { "Bold" } else { "Regular" },
                                Action::Text(Settings {
                                    bold_text,
                                    ..settings
                                }),
                                settings.bold_text == bold_text,
                            )
                        }),
                    )];
                    rows.push(ConfigRow::new(
                        "Text contrast",
                        [false, true].map(|text_contrast| {
                            Choice::option(
                                if text_contrast { "Darker" } else { "Normal" },
                                Action::Text(Settings {
                                    text_contrast,
                                    ..settings
                                }),
                                settings.text_contrast == text_contrast,
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
                            [false, true].map(|disable_soft_hyphens| {
                                Choice::option(
                                    if disable_soft_hyphens {
                                        "Off"
                                    } else {
                                        "Publisher"
                                    },
                                    Action::Text(Settings {
                                        disable_soft_hyphens,
                                        ..settings
                                    }),
                                    settings.disable_soft_hyphens == disable_soft_hyphens,
                                )
                            }),
                        ));
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
                                    prefs.zoom == 0 && !prefs.width_fit && !prefs.height_fit,
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
                                        prefs.zoom == index,
                                    )
                                }),
                        ));
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
