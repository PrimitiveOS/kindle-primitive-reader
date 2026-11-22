//! Reflow font controls, with the same indexed contrast levels as the reference reader.
use std::sync::OnceLock;

pub(crate) const NORMAL_GAMMA: u8 = 15;
pub(crate) const GAMMA_LEVELS: [f64; 57] = [
    0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 0.98, 1.0, 1.02,
    1.05, 1.1, 1.15, 1.2, 1.25, 1.3, 1.35, 1.4, 1.45, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2, 2.3,
    2.4, 2.5, 2.6, 2.7, 2.8, 2.9, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0,
    12.0, 13.0, 14.0, 15.0,
];
pub(crate) const GAMMA_PRESETS: [u8; 8] = [10, 15, 25, 30, 36, 43, 49, 56];

pub(crate) fn gamma_table(index: u8) -> &'static [u8; 256] {
    static TABLES: OnceLock<[[u8; 256]; 57]> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        std::array::from_fn(|index| {
            std::array::from_fn(|alpha| {
                let paper = (255 - alpha) as f64 / 255.0;
                255 - (paper.powf(GAMMA_LEVELS[index]) * 255.0).round() as u8
            })
        })
    });
    &tables[usize::from(index).min(GAMMA_LEVELS.len() - 1)]
}

pub(crate) fn gamma_label(index: u8) -> String {
    format!(
        "{:.2}",
        GAMMA_LEVELS[usize::from(index).min(GAMMA_LEVELS.len() - 1)]
    )
}

pub(crate) fn font_weight(publisher_weight: f32, adjustment: f32) -> f32 {
    let adjustment = if adjustment.is_finite() {
        adjustment.clamp(-3.0, 5.5)
    } else {
        0.0
    };
    (publisher_weight + (400.0 + adjustment * 100.0).floor() - 400.0).clamp(1.0, 999.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_keeps_opaque_ink_and_paper_and_neutral_coverage() {
        assert_eq!(
            gamma_table(NORMAL_GAMMA).as_slice(),
            &(0..=255).collect::<Vec<u8>>()
        );
        for index in 0..GAMMA_LEVELS.len() as u8 {
            let table = gamma_table(index);
            assert_eq!((table[0], table[255]), (0, 255));
            assert!(table.windows(2).all(|pair| pair[0] <= pair[1]));
        }
        for alpha in 0..256 {
            assert!(gamma_table(10)[alpha] <= gamma_table(15)[alpha]);
            assert!(gamma_table(15)[alpha] <= gamma_table(25)[alpha]);
        }
    }

    #[test]
    fn font_weight_adjusts_each_publisher_weight_and_clamps_the_engine_range() {
        assert_eq!(font_weight(400.0, -3.0), 100.0);
        assert_eq!(font_weight(700.0, 0.25), 725.0);
        assert_eq!(font_weight(400.0, 5.5), 950.0);
        assert_eq!(font_weight(100.0, -3.0), 1.0);
        assert_eq!(font_weight(700.0, 5.5), 999.0);
        assert_eq!(font_weight(400.0, f32::NAN), 400.0);
    }

    #[test]
    #[ignore = "requires the privately generated pinned CREngine gamma reference"]
    fn matches_all_reference_glyph_coverage_levels() {
        let path =
            std::env::var_os("READER_FONT_GAMMA_ORACLE").expect("Set READER_FONT_GAMMA_ORACLE");
        let reference = std::fs::read(path).unwrap();
        assert_eq!(reference.len(), GAMMA_LEVELS.len() * 256);
        for (index, expected) in reference.chunks_exact(256).enumerate() {
            assert_eq!(
                gamma_table(index as u8).as_slice(),
                expected,
                "gamma index {index}"
            );
        }
    }
}
