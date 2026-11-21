//! Experimental reduced IDCT on ordinary Adobe print photographs at low zoom.
use super::{ImageKind, ImageXObject};
use hayro_syntax::object::Name;
use hayro_syntax::object::dict::keys::*;
use hayro_syntax::object::stream::{FilterResult, ImageColorSpace, ImageData, ImageDecodeParams};
use jpeg_decoder::{CodingProcess, Decoder, PixelFormat};
use std::borrow::Cow;

const MAX_AREA: u32 = 24_000_000;
const MAX_COMPRESSED: usize = 16 * 1024 * 1024;
const MAX_HEADER_SEGMENTS: usize = 256;

// Accept a four-component, eight-bit baseline image carrying Adobe's CMYK or YCCK
// marker. Missing or ambiguous color metadata stays with the ordinary PDF decoder.
fn adobe_baseline(bytes: &[u8]) -> bool {
    let Some(mut rest) = bytes.strip_prefix(&[0xff, 0xd8]) else {
        return false;
    };
    let mut adobe = false;
    let mut baseline = false;
    // Large ICC profiles can occupy several APP2 segments. Skipping a segment
    // only reads its marker and length. Bound that work and the entire input.
    for _ in 0..MAX_HEADER_SEGMENTS {
        if rest.len() < 4 || rest[0] != 0xff {
            return false;
        }
        let marker = rest[1];
        let len = u16::from_be_bytes([rest[2], rest[3]]) as usize;
        if len < 2 || len + 2 > rest.len() {
            return false;
        }
        let payload = &rest[4..len + 2];
        match marker {
            // A real sequential, non-interleaved four-scan print PDF control
            // loses low-contrast detail with reduced decoding. Leave it to the
            // full decoder. An ordinary baseline scan includes all four planes.
            0xda => {
                return baseline
                    && adobe
                    && payload.len() == 12
                    && payload[0] == 4
                    && payload[9..] == [0, 63, 0];
            }
            0xee if payload.starts_with(b"Adobe") => {
                if adobe || !matches!(payload.get(11), Some(0 | 2)) {
                    return false;
                }
                adobe = true;
            }
            0xc0 => {
                if baseline || payload.first() != Some(&8) || payload.get(5) != Some(&4) {
                    return false;
                }
                baseline = true;
            }
            0xc1..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf => return false,
            _ => {}
        }
        rest = &rest[len + 2..];
    }
    false
}

pub(super) fn decode(
    obj: &ImageXObject<'_>,
    params: &ImageDecodeParams,
) -> Option<FilterResult<'static>> {
    let (target_w, target_h) = params.target_dimension?;
    let area = params.width.checked_mul(params.height)?;
    // A temporary switch lets the private measurement use one binary in both orders.
    let guard = match std::env::var("READER_EXPERIMENT_DCT").as_deref() {
        Ok("2") => 2u32,
        Ok("3") => 3,
        _ => return None,
    };
    let goal_w = target_w.checked_mul(guard)?;
    let goal_h = target_h.checked_mul(guard)?;
    if !(262_144..=MAX_AREA).contains(&area)
        || target_w == 0
        || target_h == 0
        || goal_w > params.width / 2
        || goal_h > params.height / 2
        || obj.kind != ImageKind::Image
        || obj.transfer_function.is_some()
    {
        return None;
    }
    let dict = obj.stream.dict();
    if dict.get::<Name<'_>>(FILTER)?.as_ref() != b"DCTDecode"
        || dict.get::<Name<'_>>(COLORSPACE)?.as_ref() != b"DeviceCMYK"
        || params.bpc != Some(8)
        || params.num_components != Some(4)
        || [MASK, SMASK, DECODE_PARMS, DP, DECODE, D]
            .into_iter()
            .any(|k| dict.contains_key(k))
    {
        return None;
    }
    let raw = obj.stream.raw_data();
    if raw.len() > MAX_COMPRESSED || !adobe_baseline(&raw) {
        return None;
    }
    let mut decoder = Decoder::new(raw.as_ref());
    decoder.set_max_decoding_buffer_size((MAX_AREA * 4) as usize);
    decoder.read_info().ok()?;
    let info = decoder.info()?;
    if info.pixel_format != PixelFormat::CMYK32
        || info.coding_process != CodingProcess::DctSequential
        || u32::from(info.width) != params.width
        || u32::from(info.height) != params.height
    {
        return None;
    }
    // jpeg-decoder's scale targets either axis; verify *both* after choosing a factor.
    let factor = [8u32, 4, 2]
        .into_iter()
        .find(|s| params.width.div_ceil(*s) >= goal_w && params.height.div_ceil(*s) >= goal_h)?;
    let (width, height) = decoder
        .scale(
            params.width.div_ceil(factor) as u16,
            params.height.div_ceil(factor) as u16,
        )
        .ok()?;
    if u32::from(width) < goal_w || u32::from(height) < goal_h || width == info.width {
        return None;
    }
    let mut data = decoder.decode().ok()?;
    if data.len() != usize::from(width) * usize::from(height) * 4 {
        return None;
    }
    // The PDF DCT decoder returns ink values opposite to jpeg-decoder's Adobe output.
    for component in &mut data {
        *component = 255 - *component;
    }
    if std::env::var_os("READER_PDF_OPEN_TIMINGS").is_some() {
        eprintln!(
            "reader_pdf_scaled_dct input={}x{} target={target_w}x{target_h} decoded={width}x{height}",
            params.width, params.height
        );
    }
    Some(FilterResult {
        data: Cow::Owned(data),
        image_data: Some(ImageData {
            alpha: None,
            color_space: Some(ImageColorSpace::Cmyk),
            bits_per_component: 8,
            width: u32::from(width),
            height: u32::from(height),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::adobe_baseline;

    fn header(frame: u8, precision: u8, components: u8, transform: u8) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xee, 0, 14];
        bytes.extend_from_slice(b"Adobe");
        bytes.extend_from_slice(&[0, 100, 0, 0, 0, 0, transform]);
        bytes.extend_from_slice(&[0xff, frame, 0, 8, precision, 0, 24, 0, 32, components]);
        bytes.extend_from_slice(&[0xff, 0xda, 0, 14, 4, 1, 0, 2, 0, 3, 0, 4, 0, 0, 63, 0]);
        bytes
    }

    #[test]
    fn noninterleaved_and_nonsequential_scans_keep_the_full_decoder() {
        let mut bytes = header(0xc0, 8, 4, 2);
        let sos = bytes.len() - 16;
        bytes.truncate(sos);
        bytes.extend_from_slice(&[0xff, 0xda, 0, 8, 1, 1, 0, 0, 63, 0]);
        assert!(!adobe_baseline(&bytes));

        let mut spectral = header(0xc0, 8, 4, 2);
        *spectral.last_mut().unwrap() = 1;
        assert!(!adobe_baseline(&spectral));
    }

    #[test]
    fn bounds_marker_work_while_skipping_large_profile_segments() {
        let header = header(0xc0, 8, 4, 2);
        let mut profile = header[..2].to_vec();
        for _ in 0..10 {
            profile.extend_from_slice(&[0xff, 0xe2, 0xff, 0xff]);
            profile.resize(profile.len() + 65_533, 0);
        }
        profile.extend_from_slice(&header[2..]);
        assert!(adobe_baseline(&profile));

        let mut excessive = header[..2].to_vec();
        for _ in 0..super::MAX_HEADER_SEGMENTS {
            excessive.extend_from_slice(&[0xff, 0xe2, 0, 2]);
        }
        excessive.extend_from_slice(&header[2..]);
        assert!(!adobe_baseline(&excessive));
        assert!(!adobe_baseline(&profile[..128]));
    }

    #[test]
    fn accepts_only_explicit_eight_bit_adobe_print_images() {
        for transform in [0, 2] {
            assert!(adobe_baseline(&header(0xc0, 8, 4, transform)));
        }
        for sample in [
            header(0xc2, 8, 4, 2),
            header(0xc0, 12, 4, 2),
            header(0xc0, 8, 3, 2),
            header(0xc0, 8, 4, 1),
        ] {
            assert!(!adobe_baseline(&sample));
        }
        let sample = header(0xc0, 8, 4, 2);
        for length in 0..sample.len() {
            assert!(!adobe_baseline(&sample[..length]));
        }
    }
}
