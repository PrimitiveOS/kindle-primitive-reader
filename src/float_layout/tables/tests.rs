use crate::{Anchor, Book, Engine, Probe, PublisherRenderMode, Renderer, Settings, user_styles};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 520;
const MARGIN: u32 = 40;
const CASES: [&[&str]; 7] = [
    &[
        "Left caption",
        "Port cell",
        "Below port",
        "Beside port",
        "After port",
    ],
    &[
        "Right note",
        "Quay",
        "Below quay",
        "Beside quay",
        "After quay",
    ],
    &[
        "RTL caption",
        "First cell",
        "Next cell",
        "Below RTL",
        "Beside RTL",
        "After RTL",
    ],
    &[
        "Inline caption",
        "Inline cell",
        "Inline lower",
        "Below inline",
        "After inline",
    ],
    &[
        "Lead caption",
        "Lead cell",
        "Lead lower",
        "Trail caption",
        "Trail cell",
        "Trail lower",
        "Along leading float",
        "After right clear",
    ],
    &[
        "Full caption",
        "Wide cell",
        "Wide lower",
        "Below wide float",
        "After wide float",
    ],
    &[
        "Centered caption",
        "Center cell",
        "Center lower",
        "Below centered",
        "After centered",
    ],
];

fn open(case: usize, mode: PublisherRenderMode, ignore_hints: bool, height: u32) -> Probe {
    let mut data = user_styles::Data::default();
    if ignore_hints {
        data.choose(1, "table_no_presentational_hints", Some(true))
            .unwrap();
    }
    Probe::from_book_with_rules(
        Book::open(&crate::test_fixture("floating-tables.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        mode.apply(Settings {
            width: WIDTH,
            height,
            margin: MARGIN,
            font_size: 22.0,
            ..Settings::default()
        }),
        case,
        None,
        &crate::user_hyphenation::Rules::default(),
        &data.snapshot(1).unwrap(),
    )
    .unwrap()
}

fn uses_floats(probe: &Probe) -> bool {
    matches!(&probe.renderer, Renderer::Parley(layout) if layout.uses_float_geometry())
}

fn mark(probe: &Probe, phrase: &str) -> (Anchor, usize, crate::selection::HighlightRect) {
    let matches = probe.matches(phrase);
    assert_eq!(matches.len(), 1, "{phrase}");
    let start = matches.into_iter().next().unwrap();
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + phrase.len(),
        ..start.clone()
    };
    let selection = probe.select_range(&start, &end).unwrap();
    assert_eq!(selection.text, phrase);
    let page = probe.page_for(&start).unwrap();
    let rect = probe.selection_rects(page, &selection)[0];
    (start, page, rect)
}

#[test]
fn floating_tables_respect_modes_hints_and_physical_rtl_side() {
    use PublisherRenderMode::*;
    for case in 0..CASES.len() {
        let reference = open(case, Flat, false, HEIGHT);
        let saved: Vec<_> = CASES[case]
            .iter()
            .map(|phrase| mark(&reference, phrase).0)
            .collect();
        for (mode, ignore_hints) in [
            (Legacy, false),
            (Flat, false),
            (Book, false),
            (Web, false),
            (Book, true),
        ] {
            let probe = open(case, mode, ignore_hints, HEIGHT);
            let active = matches!(mode, Book | Web);
            let floating = active
                && match case {
                    0 | 1 => !ignore_hints,
                    2 | 4 | 5 => true,
                    _ => false,
                };
            let points: Vec<_> = CASES[case]
                .iter()
                .zip(&saved)
                .map(|(phrase, before)| {
                    let (anchor, page, rect) = mark(&probe, phrase);
                    assert_eq!(anchor, *before);
                    assert_eq!(probe.restore_anchor(before), Some(before.clone()));
                    assert_eq!(page, 0);
                    rect
                })
                .collect();
            let x: Vec<_> = points.iter().map(|rect| rect.x).collect();
            let y: Vec<_> = points.iter().map(|rect| rect.y).collect();
            let info = format!("{case} {mode:?} hints={} x={x:?} y={y:?}", !ignore_hints);
            assert_eq!(uses_floats(&probe), floating, "{info}");
            match case {
                0 => {
                    assert!(y[1] < y[2] && y[2] < y[0] && y[0] < y[4], "{info}");
                    if floating {
                        assert!(x[3] == 240 && y[3] < y[1], "{info}");
                    } else {
                        assert!(y[3] > y[0] && x[3] <= x[0], "{info}");
                    }
                }
                1 | 2 => {
                    let neighbor = points.len() - 2;
                    assert!(
                        y[neighbor + 1] > y[neighbor] && y[neighbor + 1] > y[neighbor - 1],
                        "{info}"
                    );
                    if floating {
                        assert!(
                            x[0] > 320 && x[neighbor] <= 42 && y[neighbor] < y[1],
                            "{info}"
                        );
                    } else {
                        assert!(y[neighbor] > y[neighbor - 1], "{info}");
                    }
                    if case == 2 && active {
                        assert!(x[1] > x[2], "{info}");
                    }
                }
                4 => {
                    assert!(y[3] > y[2] && y[7] > y[5] && y[7] > y[6], "{info}");
                    if active {
                        assert!(x[3] > 320 && x[7] <= 42, "{info}");
                        if ignore_hints {
                            assert!(x[6] <= 42 && y[6] > y[2], "{info}");
                        } else {
                            assert!(x[6] > 180 && y[6] < y[1], "{info}");
                        }
                    }
                }
                _ => assert!(y[2] < y[3] && y[3] < y[4], "{info}"),
            }
            let caption = points[0];
            let link = (caption.y + 1..caption.y + caption.height)
                .step_by(3)
                .find_map(|y| {
                    (caption.x + 1..caption.x + caption.width)
                        .step_by(3)
                        .find_map(|x| probe.link_at(0, x as f32, y as f32))
                })
                .unwrap_or_else(|| panic!("Missing caption link at {caption:?} ({info})"));
            assert_eq!(link.fragment.as_deref(), Some("finish"), "{info}");
        }
    }
}

#[test]
fn floating_tables_preserve_paged_source_and_paint_relocated_borders() {
    use PublisherRenderMode::Book;
    for (case, label) in [(0, "left"), (2, "rtl")] {
        for ignore_hints in [false, true] {
            let mut probe = open(case, Book, ignore_hints, HEIGHT);
            let (anchor, _, caption) = mark(&probe, CASES[case][0]);
            let reopened = open(case, Book, ignore_hints, 250);
            assert_eq!(reopened.restore_anchor(&anchor), Some(anchor.clone()));
            let (same, page, _) = mark(&reopened, CASES[case][0]);
            assert_eq!(anchor, same);
            assert!(
                reopened
                    .page_range(page)
                    .contains(&reopened.anchor_top(&anchor).unwrap())
            );
            let pixels = probe.capture(0).unwrap();
            let y = (caption.y + caption.height / 2) as usize;
            let row = &pixels[y * WIDTH as usize..(y + 1) * WIDTH as usize];
            let start = if case == 0 { MARGIN as usize } else { 320 };
            let left = row[start..].iter().position(|&gray| gray == 17).unwrap() + start;
            let right = row.iter().rposition(|&gray| gray == 17).unwrap();
            if case == 0 {
                assert_eq!(left, MARGIN as usize);
            } else {
                assert_eq!(right, (WIDTH - MARGIN - 1) as usize);
            }
            assert_eq!(row[left + 2], 102);
            assert_eq!(row[right - 2], 102);
            assert!(row.contains(&221));
            if let Some(path) = std::env::var_os("READER_FLOAT_TABLE_CAPTURES") {
                let directory = std::path::PathBuf::from(path);
                std::fs::create_dir_all(&directory).unwrap();
                let suffix = if ignore_hints { "-without-hints" } else { "" };
                image::GrayImage::from_raw(WIDTH, HEIGHT, pixels)
                    .unwrap()
                    .save(directory.join(format!("reader-floating-table-{label}{suffix}.png")))
                    .unwrap();
            }
        }
    }
}
