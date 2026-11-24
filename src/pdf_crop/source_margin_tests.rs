use super::*;
use std::fmt::Write;
use std::path::PathBuf;

struct Case {
    name: &'static str,
    size: (f32, f32),
    stock: [f32; 4],
}

const UPRIGHT: (f32, f32) = (600.0, 800.0);
const CLEAN: [f32; 4] = [151.66937, 252.66937, 432.33063, 585.33063];
const SEED: [f32; 4] = [30.0, 40.0, 570.0, 760.0];
const CASES: &[Case] = &[
    Case {
        name: "clean",
        size: UPRIGHT,
        stock: CLEAN,
    },
    Case {
        name: "edge-specks",
        size: UPRIGHT,
        stock: CLEAN,
    },
    Case {
        name: "faint-shaded",
        size: UPRIGHT,
        stock: SEED,
    },
    Case {
        name: "faint-white",
        size: UPRIGHT,
        stock: SEED,
    },
    Case {
        name: "gutter",
        size: UPRIGHT,
        stock: [37.29579, 93.29578, 433.70422, 721.7042],
    },
    Case {
        name: "rotated-specks",
        size: (800.0, 600.0),
        stock: [213.03728, 151.03728, 546.9627, 432.9627],
    },
    Case {
        name: "shaded",
        size: UPRIGHT,
        stock: CLEAN,
    },
    Case {
        name: "sparse-notes",
        size: UPRIGHT,
        stock: [151.92464, 191.92464, 411.07538, 666.0754],
    },
    Case {
        name: "tiny",
        size: UPRIGHT,
        stock: SEED,
    },
];

pub(crate) fn seed(size: (f32, f32)) -> Seed {
    Seed::from_normalized(
        PdfTextRect {
            x0: 0.05,
            y0: 0.05,
            x1: 0.95,
            y1: 0.95,
        },
        size,
    )
}

pub(crate) fn fixture(name: &str) -> Vec<u8> {
    let mut content = String::new();
    if matches!(name, "shaded" | "faint-shaded") {
        content.push_str("0.9 g 0 0 600 800 re f\n");
    }
    match name {
        "tiny" => content.push_str("0 g 294 397 5 5 re f\n"),
        "sparse-notes" => content.push_str("0 g 155 600 126 5 re f 320 136 89 4 re f\n"),
        _ => {
            let level = if name.starts_with("faint") {
                "0.78"
            } else {
                "0"
            };
            for (line, width) in [249, 234, 275, 256, 270, 232, 275, 252, 273, 232]
                .into_iter()
                .enumerate()
            {
                let y = 217 + 35 * line;
                writeln!(&mut content, "{level} g 155 {y} {width} 12 re f").unwrap();
            }
        }
    }
    match name {
        "edge-specks" | "rotated-specks" => {
            content.push_str("0 g 90 639 1.3 1.3 re f 507 155 1.5 1.5 re f 120 112 1.2 1.2 re f\n")
        }
        "gutter" => content.push_str("0.65 g 42 82 18 620 re f\n"),
        _ => {}
    }
    let rotation = if name == "rotated-specks" { 90 } else { 0 };
    crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 600 800] >>".into(),
        format!("<< /Type /Page /Parent 2 0 R /Rotate {rotation} /Contents 4 0 R >>"),
        format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
    ])
}

#[test]
fn scan_margin_specks_match_the_source_geometry_controls_from_the_stock_runtime() {
    let directory = std::env::var_os("READER_SCAN_MARGIN_ORACLE_DIR").map(PathBuf::from);
    if let Some(directory) = &directory {
        std::fs::create_dir_all(directory).unwrap();
    }
    for case in CASES {
        let bytes = fixture(case.name);
        if let Some(directory) = &directory {
            std::fs::write(directory.join(format!("{}.pdf", case.name)), &bytes).unwrap();
        }
        let pdf = Pdf::open(bytes).unwrap();
        assert_eq!(pdf.page_size(0).unwrap(), case.size, "{}", case.name);
        let area = detect(&pdf, 0, Some(seed(case.size)), 1264, 10).unwrap();
        let native = Seed::from_normalized(area, case.size);
        let edges = [native.x0, native.y0, native.x1, native.y1];
        let max_difference = edges
            .into_iter()
            .zip(case.stock)
            .map(|(actual, stock)| (actual - stock).abs())
            .fold(0.0, f32::max);
        eprintln!(
            "scan-margin {} {edges:?} max_pt={max_difference:.4}",
            case.name
        );
        assert!(
            max_difference < 1.1,
            "{} differs by {max_difference} points",
            case.name
        );
    }
}

#[test]
fn scan_margin_specks_leave_thin_connected_strokes() {
    let size = 14;
    let mut raster = vec![255; size * size];
    for dot in 3..10 {
        raster[dot * size + dot] = 0;
    }
    raster[size + 9] = 0;
    raster[size + 10] = 0;
    raster[size * 12 + 2] = 0;
    discard_isolated_specks(&mut raster, size, 2.0);
    assert_eq!(raster.iter().filter(|&&pixel| pixel == 0).count(), 7);
    assert!((3..10).all(|dot| raster[dot * size + dot] == 0));
}
