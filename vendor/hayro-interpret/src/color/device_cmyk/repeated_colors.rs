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

    /// Return the number of color tuples that required the ICC transform.
    fn convert_chunk(
        &mut self,
        samples: &[u8],
        rgb: &mut [u8],
        transform: &impl Fn(&[u8], &mut [u8]) -> Option<()>,
    ) -> Option<usize> {
        self.next_chunk();
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
            let cached = self.cached[cache_slot(hashed)];
            if cached.valid && cached.key == key {
                *result = cached.rgb;
                continue;
            }

            // A hash collision may add a duplicate to this chunk's palette. A complete
            // four-byte match is required before sharing a conversion result.
            let pending = &mut self.pending[(hashed >> (32 - PENDING_SLOTS.ilog2())) as usize];
            let index = if pending.generation == self.generation && pending.key == key {
                pending.index
            } else {
                let index = unique as u16;
                self.samples[unique] = *pixel;
                *pending = PendingColor {
                    key,
                    generation: self.generation,
                    index,
                };
                unique += 1;
                index
            };
            self.writes[writes] = (position as u16, index);
            writes += 1;
        }
        if unique != 0 {
            transform(
                self.samples[..unique].as_flattened(),
                self.rgb[..unique].as_flattened_mut(),
            )?;
            for (sample, result) in self.samples[..unique].iter().zip(self.rgb.iter()) {
                let key = u32::from_le_bytes(*sample);
                self.cached[cache_slot(hash(key))] = CachedColor {
                    key,
                    rgb: *result,
                    valid: true,
                };
            }
            let pixels = rgb.as_chunks_mut::<3>().0;
            for &(position, index) in &self.writes[..writes] {
                pixels[usize::from(position)] = self.rgb[usize::from(index)];
            }
        }
        Some(unique)
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
    let mut transformed = 0;
    for (i, (samples, rgb)) in input
        .chunks(CHUNK * 4)
        .zip(output.chunks_mut(CHUNK * 3))
        .enumerate()
    {
        transformed += palette.convert_chunk(samples, rgb, &transform)?;
        let total = (i + 1) * CHUNK;
        if total == PROBE_PIXELS && transformed > total * 9 / 10 && total * 4 < input.len() {
            return transform(&input[total * 4..], &mut output[total * 3..]);
        }
    }
    Some(())
}

/// Compact the owned decoder buffer from four components per pixel to three.
/// A completed block ends before the next unread CMYK pixel in the same allocation.
pub(super) fn compact(
    data: &mut Vec<u8>,
    transform: impl Fn(&[u8], &mut [u8]) -> Option<()>,
) -> Option<()> {
    if !data.len().is_multiple_of(4) {
        return None;
    }
    let pixels = data.len() / 4;
    let mut palette = (pixels >= PROBE_PIXELS).then(Palette::new).flatten();
    let mut transformed = 0;
    let mut scratch = [0_u8; CHUNK * 3];
    for start in (0..pixels).step_by(CHUNK) {
        let end = pixels.min(start + CHUNK);
        let input = &data[start * 4..end * 4];
        let rgb = &mut scratch[..(end - start) * 3];
        if let Some(colors) = &mut palette {
            transformed += colors.convert_chunk(input, rgb, &transform)?;
            if end == PROBE_PIXELS && transformed > end * 9 / 10 {
                palette = None;
            }
        } else {
            transform(input, rgb)?;
        }
        data[start * 3..end * 3].copy_from_slice(rgb);
    }
    data.truncate(pixels * 3);
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
    fn compacted_rows_match_every_original_icc_component_with_the_same_allocation() {
        // The unusual row width puts chunk boundaries inside rows rather than at their edges.
        let width = 509;
        for pixels in [
            0,
            1,
            width,
            CHUNK - 1,
            CHUNK,
            CHUNK + 1,
            PROBE_PIXELS,
            width * 233,
        ] {
            let samples: Vec<[u8; 4]> = (0..pixels)
                .map(|i| {
                    let (row, column) = (i / width, i % width);
                    let stripe = ((column / 7 + row / 3) % 89) as u8;
                    [
                        stripe,
                        stripe.rotate_left(1),
                        row as u8,
                        (column / 11) as u8,
                    ]
                })
                .collect();
            let original = samples.as_flattened();
            let mut compacted = original.to_vec();
            let allocation = (compacted.as_ptr(), compacted.capacity());
            let mut expected = vec![0u8; pixels * 3];
            if pixels > 0 {
                super::super::CMYK_TRANSFORM
                    .convert(original, &mut expected)
                    .unwrap();
            }
            compact(&mut compacted, |input, output| {
                super::super::CMYK_TRANSFORM.convert(input, output)
            })
            .unwrap();
            assert_eq!((compacted.as_ptr(), compacted.capacity()), allocation);
            assert_eq!(compacted.len(), pixels * 3);
            for (actual, reference) in compacted.chunks(width * 3).zip(expected.chunks(width * 3)) {
                assert_eq!(actual, reference);
            }
        }
    }

    #[test]
    fn in_place_high_entropy_tail_remains_in_original_order_after_the_cache_probe() {
        let pixels = PROBE_PIXELS + CHUNK * 2 + 13;
        let samples: Vec<[u8; 4]> = (0..pixels)
            .map(|index| (index as u32).wrapping_mul(0xb16f_3565).to_le_bytes())
            .collect();
        let original = samples.as_flattened();
        let mut compacted = original.to_vec();
        let direct_offset = Cell::new(0);
        let calls = Cell::new(0);
        compact(&mut compacted, |input, rgb| {
            if calls.get() >= PROBE_PIXELS / CHUNK {
                let start = (PROBE_PIXELS + direct_offset.get()) * 4;
                assert_eq!(input, &original[start..start + input.len()]);
                direct_offset.set(direct_offset.get() + input.len() / 4);
            }
            calls.set(calls.get() + 1);
            for (sample, output) in input
                .as_chunks::<4>()
                .0
                .iter()
                .zip(rgb.as_chunks_mut::<3>().0)
            {
                *output = [sample[0] ^ sample[3], sample[2], sample[1]];
            }
            Some(())
        })
        .unwrap();
        assert_eq!(direct_offset.get(), pixels - PROBE_PIXELS);
        for (sample, actual) in samples.iter().zip(compacted.as_chunks::<3>().0) {
            assert_eq!(*actual, [sample[0] ^ sample[3], sample[2], sample[1]]);
        }
    }

    #[test]
    fn compact_rejects_partial_pixels_and_preserves_unread_cmyk_on_failure() {
        let mut partial = vec![9; 7];
        assert!(compact(&mut partial, |_, _| panic!("malformed input converted")).is_none());
        assert_eq!(partial, vec![9; 7]);
        let mut data = vec![213; (CHUNK + 17) * 4];
        let original = data.clone();
        let calls = Cell::new(0);
        assert!(
            compact(&mut data, |input, rgb| {
                assert!(input.iter().all(|component| *component == 213));
                calls.set(calls.get() + 1);
                if calls.get() == 2 {
                    return None;
                }
                rgb.fill(61);
                Some(())
            })
            .is_none()
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(data.len(), original.len());
        assert_eq!(&data[CHUNK * 3..], &original[CHUNK * 3..]);
    }

    #[test]
    fn a_failed_icc_transform_is_reported() {
        assert!(convert(&[0; 4], &mut [0; 3], |_, _| None).is_none());
    }
}
