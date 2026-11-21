//! Memoize exact four-channel input values when converting large printer-color images.
//! The ICC transform and output colors are identical to direct conversion.
const CHUNK: usize = 8192;
const CACHE_SLOTS: usize = 1 << 18;
const PENDING_SLOTS: usize = 1 << 15;
const PROBE_PIXELS: usize = CHUNK * 8;

pub(super) fn is_candidate(samples: &[u8], output: &[u8]) -> bool {
    samples.len() >= PROBE_PIXELS * 4
        && samples.len().is_multiple_of(4)
        && output.len() == samples.len() / 4 * 3
}

#[derive(Clone, Copy, Default)]
struct CachedColor {
    key: u32,
    rgb: [u8; 3],
    valid: bool,
}

#[derive(Clone, Copy, Default)]
struct PendingColor {
    key: u32,
    generation: u16,
    index: u16,
}

fn filled<T: Clone + Default>(length: usize) -> Option<Vec<T>> {
    let mut entries = Vec::new();
    entries.try_reserve_exact(length).ok()?;
    entries.resize(length, T::default());
    Some(entries)
}

struct Palette {
    cached: Vec<CachedColor>,
    pending: Vec<PendingColor>,
    samples: Vec<[u8; 4]>,
    rgb: Vec<[u8; 3]>,
    writes: Vec<(u16, u16)>,
    generation: u16,
}

impl Palette {
    fn new() -> Option<Self> {
        Some(Self {
            cached: filled(CACHE_SLOTS)?,
            pending: filled(PENDING_SLOTS)?,
            samples: filled(CHUNK)?,
            rgb: filled(CHUNK)?,
            writes: filled(CHUNK)?,
            generation: 0,
        })
    }

    fn next_chunk(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.pending.fill(PendingColor::default());
            self.generation = 1;
        }
    }
}

fn hash(key: u32) -> u32 {
    (key ^ (key >> 16)).wrapping_mul(0x9e37_79b1)
}

fn cache_slot(hash: u32) -> usize {
    (hash >> (32 - CACHE_SLOTS.ilog2())) as usize
}

pub(super) fn convert(
    input: &[u8],
    output: &mut [u8],
    transform: impl Fn(&[u8], &mut [u8]) -> Option<()>,
) -> Option<()> {
    let Some(mut palette) = Palette::new() else {
        return transform(input, output);
    };
    let mut total = 0;
    let mut transformed = 0;
    while total < input.len() / 4 {
        let count = CHUNK.min(input.len() / 4 - total);
        let samples = &input[total * 4..(total + count) * 4];
        let rgb = &mut output[total * 3..(total + count) * 3];
        palette.next_chunk();
        let mut unique = 0;
        let mut writes = 0;
        for (position, (pixel, result)) in samples
            .as_chunks::<4>()
            .0
            .iter()
            .zip(rgb.as_chunks_mut::<3>().0.iter_mut())
            .enumerate()
        {
            let key = u32::from_le_bytes(*pixel);
            let hashed = hash(key);
            let cached = palette.cached[cache_slot(hashed)];
            if cached.valid && cached.key == key {
                *result = cached.rgb;
                continue;
            }

            // A hash collision may add a duplicate to this chunk's palette. A complete
            // four-byte match is required before sharing a conversion result.
            let pending = &mut palette.pending[(hashed >> (32 - PENDING_SLOTS.ilog2())) as usize];
            let index = if pending.generation == palette.generation && pending.key == key {
                pending.index
            } else {
                let index = unique as u16;
                palette.samples[unique] = *pixel;
                *pending = PendingColor {
                    key,
                    generation: palette.generation,
                    index,
                };
                unique += 1;
                index
            };
            palette.writes[writes] = (position as u16, index);
            writes += 1;
        }
        if unique != 0 {
            transform(
                palette.samples[..unique].as_flattened(),
                palette.rgb[..unique].as_flattened_mut(),
            )?;
            for (sample, result) in palette.samples[..unique].iter().zip(palette.rgb.iter()) {
                let key = u32::from_le_bytes(*sample);
                palette.cached[cache_slot(hash(key))] = CachedColor {
                    key,
                    rgb: *result,
                    valid: true,
                };
            }
            let pixels = rgb.as_chunks_mut::<3>().0;
            for &(position, index) in &palette.writes[..writes] {
                pixels[usize::from(position)] = palette.rgb[usize::from(index)];
            }
        }
        total += count;
        transformed += unique;
        // A high-entropy photograph may offer little reuse. Bound the extra work on it.
        if total == PROBE_PIXELS && transformed > total * 9 / 10 {
            return if total * 4 == input.len() {
                Some(())
            } else {
                transform(&input[total * 4..], &mut output[total * 3..])
            };
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ToRgb;
    use std::cell::Cell;

    #[test]
    fn a_generated_print_chart_matches_the_reference_profile_for_every_rgb_component() {
        let buffer_bytes = CACHE_SLOTS * size_of::<CachedColor>()
            + PENDING_SLOTS * size_of::<PendingColor>()
            + CHUNK * (size_of::<[u8; 4]>() + size_of::<[u8; 3]>() + size_of::<(u16, u16)>());
        assert_eq!(buffer_bytes, 2_449_408);
        let samples: Vec<[u8; 4]> = (0..CHUNK * 12 + 317)
            .map(|p| {
                let (x, y) = (p % 509, p / 509);
                let strip = ((x / 9 + y / 6) & 63) as u8;
                [
                    strip.wrapping_mul(5),
                    strip.rotate_left(2),
                    (y / 11) as u8,
                    (x / 23) as u8,
                ]
            })
            .collect();
        let input = samples.as_flattened();
        let mut expected = vec![0u8; samples.len() * 3];
        let mut result = vec![0u8; expected.len()];
        super::super::CMYK_TRANSFORM
            .convert(input, &mut expected)
            .unwrap();
        let total = Cell::new(0);
        assert!(is_candidate(input, &result));
        convert(input, &mut result, |colors, rgb| {
            total.set(total.get() + colors.len() / 4);
            super::super::CMYK_TRANSFORM.convert(colors, rgb)
        })
        .unwrap();
        assert_eq!(result, expected);
        assert!(total.get() < samples.len() / 2);
        assert!(!is_candidate(&input[..7], &result));
        assert!(!is_candidate(input, &result[1..]));
    }

    #[test]
    fn high_entropy_falls_back_to_the_original_transform_after_a_bounded_probe() {
        let pixels = PROBE_PIXELS * 3 + 271;
        let samples: Vec<[u8; 4]> = (0..pixels)
            .map(|n| (n as u32).wrapping_mul(0xb16f_3565).to_le_bytes())
            .collect();
        let mut output = vec![0u8; pixels * 3];
        let largest = Cell::new(0);
        convert(samples.as_flattened(), &mut output, |input, output| {
            largest.set(largest.get().max(input.len() / 4));
            for (source, result) in input
                .as_chunks::<4>()
                .0
                .iter()
                .zip(output.as_chunks_mut::<3>().0)
            {
                *result = [source[0] ^ source[3], source[1], source[2]];
            }
            Some(())
        })
        .unwrap();
        assert_eq!(largest.get(), pixels - PROBE_PIXELS);
        for (pixel, result) in samples.iter().zip(output.as_chunks::<3>().0) {
            assert_eq!(*result, [pixel[0] ^ pixel[3], pixel[1], pixel[2]]);
        }
    }

    #[test]
    fn a_failed_icc_transform_is_reported() {
        assert!(convert(&[0; 4], &mut [0; 3], |_, _| None).is_none());
    }
}
