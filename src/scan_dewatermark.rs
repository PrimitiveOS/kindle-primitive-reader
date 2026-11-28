//! Bounded automatic paper cleanup for fixed-page scans.
//!
//! KOReader delegates its Dewatermark option to k2pdfopt's page optimizer. This
//! module is an independent native approximation: it selects a global
//! between-class grayscale threshold and treats lighter pixels as paper. It
//! never modifies the source page or its coordinate geometry.

const MAX_SAMPLES: usize = 1_000_000;

/// Return an Otsu-style threshold for a grayscale page, after an optional
/// display tone mapping. A uniform page has no useful split and returns None.
pub(crate) fn threshold(pixels: &[u8], tone_map: Option<&[u8; 256]>) -> Option<u8> {
    if pixels.len() < 2 {
        return None;
    }

    let stride = pixels.len().div_ceil(MAX_SAMPLES).max(1);
    let mut histogram = [0_u64; 256];
    let mut count = 0_u64;
    let mut total = 0_u64;
    for &pixel in pixels.iter().step_by(stride) {
        let tone = tone_map.map_or(pixel, |map| map[usize::from(pixel)]);
        histogram[usize::from(tone)] += 1;
        count += 1;
        total += u64::from(tone);
    }
    if count < 2 {
        return None;
    }

    let mut lower_count = 0_u64;
    let mut lower_sum = 0_u64;
    let mut best_score = -1.0_f64;
    let mut first_best = 0_u8;
    let mut last_best = 0_u8;
    for split in 0_u16..=254 {
        let tone = split as usize;
        lower_count += histogram[tone];
        lower_sum += histogram[tone] * u64::from(split as u8);
        let upper_count = count - lower_count;
        if lower_count == 0 || upper_count == 0 {
            continue;
        }

        // Between-class variance, expressed without calculating the two
        // means separately.  The histogram and sample count are bounded, so
        // f64 safely covers these integer magnitudes.
        let delta = (lower_sum * count) as f64 - (total * lower_count) as f64;
        let score = delta * delta / (lower_count as f64 * upper_count as f64);
        let tolerance = best_score.abs().max(1.0) * 1.0e-12;
        if score > best_score + tolerance {
            best_score = score;
            first_best = split as u8;
            last_best = split as u8;
        } else if (score - best_score).abs() <= tolerance {
            // A gap between two histogram modes often produces several
            // equally good thresholds. Use its midpoint instead of clipping
            // every antialiased tone above the first mode.
            last_best = split as u8;
        }
    }

    (best_score >= 0.0).then(|| first_best + (last_best - first_best) / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_midpoint_between_ink_and_paper_modes() {
        let mut page = vec![28; 20_000];
        page.extend(vec![224; 80_000]);
        let threshold = threshold(&page, None).unwrap();
        assert!((28..224).contains(&threshold));
        assert_eq!(threshold, 125);
    }

    #[test]
    fn applies_the_display_tone_map_when_estimating_threshold() {
        let mut page = vec![20; 5_000];
        page.extend(vec![140; 15_000]);
        let mut tones = std::array::from_fn(|tone| tone as u8);
        tones[140] = 220;
        assert_eq!(threshold(&page, Some(&tones)), Some(119));
    }

    #[test]
    fn uniform_or_too_small_pages_have_no_automatic_threshold() {
        assert_eq!(threshold(&[255; 128], None), None);
        assert_eq!(threshold(&[0], None), None);
    }
}
