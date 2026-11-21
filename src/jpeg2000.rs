//! Bounded JPEG 2000 Part 1 still pages for Kindle, including raw codestreams.
use crate::Result;
use hayro_jpeg2000::{ColorSpace, DecodeSettings, DecoderContext, Image};
use image::{GrayImage, Pixel, Rgb};

pub(crate) const MAX_INPUT: u64 = 8 * 1024 * 1024;
const MAX_PIXELS: u64 = 4 * 1024 * 1024;
const MAX_SAMPLES: u64 = 8 * 1024 * 1024;
const MAX_TILES: u32 = 1024;
const MAX_BOXES: usize = 256;
const MAX_METADATA: usize = 1024 * 1024;
const JP2: &[u8] = b"\x00\x00\x00\x0cjP  \r\n\x87\n";
const J2K: &[u8] = b"\xff\x4f\xff\x51";

pub(crate) fn has_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(JP2) || bytes.starts_with(J2K)
}

fn number(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}

type BoxHeader<'a> = (&'a [u8], &'a [u8]);

/// Consume one boxed region, including extended and EOF lengths. A contained box
/// must end within the enclosing region before it is handed to the codec.
fn next_box<'a>(remaining: &mut &'a [u8]) -> Result<BoxHeader<'a>> {
    let header = remaining.get(..8).ok_or("Incomplete JPEG 2000 box")?;
    let (length, start) = match number(&header[..4]) {
        0 => (remaining.len(), 8),
        1 => {
            let extended = remaining
                .get(8..16)
                .ok_or("Incomplete JPEG 2000 extended box")?;
            (
                usize::try_from(u64::from_be_bytes(extended.try_into()?))?,
                16,
            )
        }
        length => (length as usize, 8),
    };
    let payload = remaining
        .get(start..length)
        .ok_or("Invalid JPEG 2000 box length")?;
    *remaining = &remaining[length..];
    Ok((&header[4..8], payload))
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Size {
    width: u32,
    height: u32,
    channels: u16,
    depth_minus_one: u8,
}

/// Main-header markers carry 16-bit lengths, except the tile boundary. Walk
/// their lengths before the codec parses optional tables and comments.
fn check_main_header(bytes: &[u8], mut offset: usize) -> Result<()> {
    for _ in 0..MAX_BOXES {
        if offset > MAX_METADATA {
            return Err("This JPEG 2000 codestream exceeds Reader's metadata limit".into());
        }
        let marker = bytes
            .get(offset..offset + 4)
            .ok_or("Incomplete JPEG 2000 main header")?;
        if marker[..2] == [0xff, 0x90] {
            return Ok(());
        }
        if marker[0] != 0xff {
            return Err("Invalid JPEG 2000 main-header marker".into());
        }
        let length = usize::from(u16::from_be_bytes(marker[2..4].try_into()?));
        if length < 2 {
            return Err("Invalid JPEG 2000 main-header length".into());
        }
        offset += 2 + length;
    }
    Err("Too many JPEG 2000 main-header markers".into())
}

/// Read SIZ before the decoder constructs tile, code-block or component buffers.
fn codestream_size(bytes: &[u8]) -> Result<Size> {
    let header = bytes
        .get(..42)
        .filter(|h| h.starts_with(J2K))
        .ok_or("Missing JPEG 2000 size marker")?;
    let channels = u16::from_be_bytes(header[40..42].try_into()?);
    let marker_size = usize::from(u16::from_be_bytes(header[4..6].try_into()?));
    if !matches!(channels, 1 | 3) || marker_size != 38 + 3 * usize::from(channels) {
        return Err("Reader supports one grayscale or three color JPEG 2000 components".into());
    }
    let right = number(&header[8..12]);
    let bottom = number(&header[12..16]);
    let left = number(&header[16..20]);
    let top = number(&header[20..24]);
    let width = right
        .checked_sub(left)
        .ok_or("Invalid JPEG 2000 image bounds")?;
    let height = bottom
        .checked_sub(top)
        .ok_or("Invalid JPEG 2000 image bounds")?;
    let pixels = u64::from(width) * u64::from(height);
    if !crate::comic::image_dimensions_valid(width, height)
        || pixels > MAX_PIXELS
        || pixels * u64::from(channels) > MAX_SAMPLES
        || right > 65_535
        || bottom > 65_535
    {
        return Err("This JPEG 2000 page exceeds Reader's pixel or component limits".into());
    }
    let tile_axis = |origin, start, end, size| -> Result<u32> {
        if size == 0 || size > 65_535 || origin > start || start - origin >= size {
            return Err("Invalid JPEG 2000 tile geometry".into());
        }
        Ok(u32::div_ceil(end - origin, size))
    };
    let across = tile_axis(
        number(&header[32..36]),
        left,
        right,
        number(&header[24..28]),
    )?;
    let down = tile_axis(
        number(&header[36..40]),
        top,
        bottom,
        number(&header[28..32]),
    )?;
    if across.saturating_mul(down) > MAX_TILES {
        return Err("This JPEG 2000 page exceeds Reader's tile limit".into());
    }
    let specs = bytes
        .get(42..4 + marker_size)
        .ok_or("Incomplete JPEG 2000 components")?;
    let depth = specs[0];
    if depth > 15
        || specs[1..3] != [1, 1]
        || specs
            .chunks_exact(3)
            .any(|c| c[0] != depth || !(1..=4).contains(&c[1]) || !(1..=4).contains(&c[2]))
    {
        return Err("Unsupported JPEG 2000 sample depth or sampling grid".into());
    }
    check_main_header(bytes, 4 + marker_size)?;
    Ok(Size {
        width,
        height,
        channels,
        depth_minus_one: depth,
    })
}

fn image_header(mut boxes: &[u8]) -> Result<Size> {
    let mut size = None;
    let mut color = None;
    let mut channel_depths = None;
    let mut count = 0;
    while !boxes.is_empty() {
        count += 1;
        if count > MAX_BOXES {
            return Err("Too many JPEG 2000 header boxes".into());
        }
        let (kind, body) = next_box(&mut boxes)?;
        match kind {
            b"ihdr" if body.len() == 14 && size.is_none() && body[11] == 7 => {
                size = Some(Size {
                    height: number(&body[..4]),
                    width: number(&body[4..8]),
                    channels: u16::from_be_bytes(body[8..10].try_into()?),
                    depth_minus_one: body[10],
                });
            }
            b"ihdr" => return Err("Invalid JPEG 2000 image header".into()),
            b"bpcc" if body.len() <= 3 && channel_depths.is_none() => channel_depths = Some(body),
            b"bpcc" => return Err("Invalid JPEG 2000 component depth metadata".into()),
            b"colr" if body.len() == 7 && body[0] == 1 && color.is_none() => {
                color = Some(match number(&body[3..7]) {
                    16 | 18 => 3,
                    17 => 1,
                    _ => return Err("Unsupported JPEG 2000 color space".into()),
                });
            }
            b"colr" | b"pclr" | b"cmap" => {
                return Err("Reader supports grayscale, sRGB and sYCC JPEG 2000 pages".into());
            }
            b"cdef" => {
                let n = body
                    .get(..2)
                    .ok_or("Invalid JPEG 2000 channel definitions")?;
                let n = usize::from(u16::from_be_bytes(n.try_into()?));
                if !matches!(n, 1 | 3)
                    || body.len() != 2 + 6 * n
                    || Some(n) != size.map(|s| usize::from(s.channels))
                    || body[2..]
                        .chunks_exact(6)
                        .enumerate()
                        .any(|(i, c)| c != [0, i as u8, 0, 0, 0, i as u8 + 1])
                {
                    return Err("Reader supports opaque JPEG 2000 color channels in order".into());
                }
            }
            _ => {}
        }
    }
    let mut size = size
        .filter(|s| Some(s.channels) == color)
        .ok_or("Missing or inconsistent JPEG 2000 color and image headers")?;
    if size.depth_minus_one == 255 {
        let channels = channel_depths
            .filter(|c| c.len() == usize::from(size.channels))
            .ok_or("Missing JPEG 2000 component depths")?;
        if !channels.iter().all(|&d| d == channels[0]) {
            return Err("Reader supports equal JPEG 2000 component depths".into());
        }
        size.depth_minus_one = channels[0];
    } else if channel_depths.is_some() {
        return Err("Inconsistent JPEG 2000 component depth metadata".into());
    }
    Ok(size)
}

fn checked_size(bytes: &[u8]) -> Result<Size> {
    if bytes.len() as u64 > MAX_INPUT {
        return Err("This JPEG 2000 file exceeds Reader's input limit".into());
    }
    if bytes.starts_with(J2K) {
        return codestream_size(bytes);
    }
    let mut boxes = bytes
        .strip_prefix(JP2)
        .ok_or("Invalid JPEG 2000 signature")?;
    let mut header = None;
    let mut data = None;
    let mut metadata = 0;
    let mut count = 0;
    let mut compatible = false;
    while !boxes.is_empty() {
        count += 1;
        if count > MAX_BOXES {
            return Err("Too many JPEG 2000 boxes".into());
        }
        let (kind, body) = next_box(&mut boxes)?;
        if kind != b"jp2c" {
            metadata += body.len();
            if metadata > MAX_METADATA {
                return Err("This JPEG 2000 file exceeds Reader's metadata limit".into());
            }
        }
        match kind {
            b"ftyp" if !compatible && body.len() >= 12 && body.starts_with(b"jp2 ") => {
                compatible = true
            }
            b"ftyp" => return Err("Reader supports single-image JP2 containers".into()),
            b"jp2h" if header.is_none() => header = Some(image_header(body)?),
            b"jp2c" if data.is_none() => data = Some(codestream_size(body)?),
            b"jp2h" | b"jp2c" => return Err("Reader supports one JPEG 2000 image per file".into()),
            _ => {}
        }
    }
    header
        .filter(|s| compatible && Some(*s) == data)
        .ok_or("Missing or inconsistent JPEG 2000 dimensions".into())
}

pub(crate) fn decode(bytes: &[u8]) -> Result<GrayImage> {
    let size = checked_size(bytes)?;
    let settings = DecodeSettings {
        strict: true,
        ..Default::default()
    };
    let image = Image::new(bytes, &settings)?;
    if image.width() != size.width
        || image.height() != size.height
        || image.original_bit_depth() != size.depth_minus_one + 1
        || image.has_alpha()
        || !matches!(
            (image.color_space(), size.channels),
            (ColorSpace::Gray, 1) | (ColorSpace::RGB, 3)
        )
    {
        return Err("Unsupported JPEG 2000 output geometry or color space".into());
    }
    let mut ctx = DecoderContext::default();
    let decoded = image.decode(&mut ctx)?;
    let pixels = (size.width * size.height) as usize;
    if decoded.components().len() != usize::from(size.channels)
        || decoded
            .components()
            .iter()
            .any(|c| c.samples().len() != pixels)
    {
        return Err("The JPEG 2000 components do not fill the page".into());
    }
    let bytes = decoded.data_u8();
    let gray = if size.channels == 1 {
        bytes
    } else {
        bytes
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&c| Rgb(c).to_luma().0[0])
            .collect()
    };
    GrayImage::from_raw(size.width, size.height, gray).ok_or("Invalid JPEG 2000 pixel count".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> Vec<u8> {
        std::fs::read(crate::test_fixture(name)).unwrap()
    }

    #[test]
    fn jpeg2000_lossless_grayscale_and_tiled_rgb_match_independent_openjpeg_pixels() {
        for (file, digest) in [
            ("harbor-beacon.jp2", "e88119e8764b41d7a9551ec64c869b32"),
            ("signal-flags.j2k", "bb2596aaedfb1c549776e85f297d4514"),
        ] {
            let decoded = decode(&sample(file)).unwrap();
            assert_eq!(decoded.dimensions(), (160, 112));
            assert_eq!(
                format!("{:x}", md5::compute(decoded.as_raw())),
                digest,
                "{file}"
            );
        }
    }

    #[test]
    fn jpeg2000_high_precision_and_lossy_subsampled_color_match_openjpeg_reference() {
        let high = decode(&sample("evening-light.jp2")).unwrap();
        assert_eq!(
            format!("{:x}", md5::compute(high.as_raw())),
            "93eb9ef20f276b2081101afd3239af8a"
        );
        let lossy = decode(&sample("harbor-lights.jp2")).unwrap();
        let reference =
            image::ImageReader::open(crate::test_fixture("harbor-lights-reference.pgm"))
                .unwrap()
                .decode()
                .unwrap()
                .into_luma8();
        assert_eq!(lossy.dimensions(), reference.dimensions());
        let differences: Vec<_> = lossy
            .iter()
            .zip(reference.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .collect();
        let maximum = differences.iter().copied().max().unwrap();
        let total: u32 = differences.iter().map(|&delta| u32::from(delta)).sum();
        assert!(
            maximum <= 3,
            "worst 8-bit luminance difference {maximum}; total {total}"
        );
        assert!(total <= differences.len() as u32 * 2);
    }

    #[test]
    fn jpeg2000_standalone_and_comic_pages_render_the_same_pixels_in_natural_order() {
        let comic = crate::comic::Comic::open(&crate::test_fixture("harbor-signals.cbz")).unwrap();
        assert_eq!(comic.page_count(), 2);
        for (i, file) in ["harbor-beacon.jp2", "signal-flags.j2k"]
            .into_iter()
            .enumerate()
        {
            let still = crate::comic::ImagePage::open(&crate::test_fixture(file)).unwrap();
            let page = comic.decode_page(i).unwrap();
            assert_eq!(page.page_size(), (160.0, 112.0));
            assert_eq!(
                page.fit(160, 112, 0).unwrap(),
                still.fit(160, 112, 0).unwrap()
            );
            assert_eq!(
                page.fit(112, 160, 1).unwrap(),
                still.fit(112, 160, 1).unwrap()
            );
        }
    }

    #[test]
    fn jpeg2000_checks_plane_dimensions_sampling_tiles_and_metadata_before_decode() {
        let original = sample("signal-flags.j2k");
        for (offset, value) in [(8, 9_000u32), (12, 0), (16, 180), (24, 0), (32, 10)] {
            let mut bytes = original.clone();
            bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
            assert!(codestream_size(&bytes).is_err());
        }
        let mut dense = original.clone();
        dense[24..28].copy_from_slice(&1u32.to_be_bytes());
        dense[28..32].copy_from_slice(&1u32.to_be_bytes());
        assert!(codestream_size(&dense).is_err());
        let mut excessive = original.clone();
        excessive[8..12].copy_from_slice(&2048u32.to_be_bytes());
        excessive[12..16].copy_from_slice(&2048u32.to_be_bytes());
        assert!(codestream_size(&excessive).is_err());
        for (offset, value) in [(41, 24), (42, 135), (45, 20), (43, 0), (46, 9)] {
            let mut changed = original.clone();
            changed[offset] = value;
            assert!(codestream_size(&changed).is_err());
        }

        let jp2 = sample("harbor-beacon.jp2");
        let header = jp2.windows(4).position(|b| b == b"ihdr").unwrap() + 4;
        let color = jp2.windows(4).position(|b| b == b"colr").unwrap() + 4;
        for (offset, value) in [
            (header + 3, 109),
            (header + 9, 4),
            (header + 10, 2),
            (color, 2),
            (color + 6, 12),
        ] {
            let mut changed = jp2.clone();
            changed[offset] = value;
            assert!(checked_size(&changed).is_err());
        }
        for length in [0, 12, 42, jp2.len() / 2] {
            assert!(decode(&jp2[..length]).is_err());
        }
        let mut extra = jp2.clone();
        extra.extend_from_slice(&jp2[jp2.windows(4).position(|b| b == b"jp2c").unwrap() - 4..]);
        assert!(checked_size(&extra).is_err());
    }

    #[test]
    fn jpeg2000_rejects_impossible_container_lengths_and_bounds_metadata() {
        let mut bytes = JP2.to_vec();
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(b"free");
        bytes.extend_from_slice(&u64::MAX.to_be_bytes());
        assert!(checked_size(&bytes).is_err());
        let mut bytes = JP2.to_vec();
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(b"xml ");
        bytes.resize(bytes.len() + MAX_METADATA + 1, 0);
        assert!(
            checked_size(&bytes)
                .unwrap_err()
                .to_string()
                .contains("metadata limit")
        );
        bytes.resize(MAX_INPUT as usize + 1, 0);
        assert!(
            checked_size(&bytes)
                .unwrap_err()
                .to_string()
                .contains("input limit")
        );
    }
}
