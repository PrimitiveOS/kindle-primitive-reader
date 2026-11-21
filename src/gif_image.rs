//! A bounded first GIF frame at its offset on the logical canvas.
use crate::{Result, comic};
use image::{GrayImage, ImageEncoder, Pixel};
use std::{io::Cursor, num::NonZeroU64};

pub(crate) fn has_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

pub(crate) fn dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    if !has_signature(bytes) || bytes.len() < 13 || bytes.len() as u64 > comic::MAX_IMAGE_BYTES {
        return Err("The GIF header or file length is unsupported".into());
    }
    let width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
    let height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
    if !comic::image_dimensions_valid(width, height) {
        return Err("This GIF exceeds Reader's image limits".into());
    }
    Ok((width, height))
}

/// Each target uses one logical canvas and an indexed frame. A transparent pixel
/// leaves paper white in a book and exposes the underlying drawing inside SVG.
fn pixels<const N: usize>(
    data: &[u8],
    clear: [u8; N],
    color: impl Fn(u8) -> [u8; N],
) -> Result<(u32, u32, Vec<u8>)> {
    let (width, height) = dimensions(data)?;
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        NonZeroU64::new(comic::MAX_IMAGE_PIXELS).unwrap(),
    ));
    options.check_frame_consistency(true);
    let mut decoder = options.read_info(Cursor::new(data))?;
    let palette = decoder.global_palette().map(<[u8]>::to_vec);
    let frame = decoder
        .read_next_frame()?
        .ok_or("This GIF has no image frame")?;
    let rgb = frame
        .palette
        .as_deref()
        .or(palette.as_deref())
        .ok_or("This GIF has no color palette")?;
    let colors: Vec<_> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&c| color(image::Rgb(c).to_luma().0[0]))
        .collect();
    let (stride, rows) = (width as usize, height as usize);
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    let (w, h) = (usize::from(frame.width), usize::from(frame.height));
    if w == 0 || h == 0 || left + w > stride || top + h > rows || frame.buffer.len() != w * h {
        return Err("This GIF has an invalid image frame".into());
    }
    let mut canvas = vec![clear; stride * rows];
    for (y, src) in frame.buffer.chunks_exact(w).enumerate() {
        let offset = (top + y) * stride + left;
        for (&index, pixel) in src.iter().zip(&mut canvas[offset..offset + w]) {
            if frame.transparent != Some(index) {
                *pixel = *colors
                    .get(usize::from(index))
                    .ok_or("This GIF contains an invalid color index")?;
            }
        }
    }
    Ok((width, height, canvas.into_flattened()))
}

pub(crate) fn gray(bytes: &[u8]) -> Result<GrayImage> {
    let (width, height, pixels) = pixels(bytes, [255], |ink| [ink])?;
    GrayImage::from_raw(width, height, pixels)
        .ok_or_else(|| "GIF pixel dimensions are inconsistent".into())
}

pub(crate) fn transparent_png(bytes: &[u8]) -> Result<Vec<u8>> {
    let (width, height, pixels) = pixels(bytes, [0, 0], |ink| [ink, 255])?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        &pixels,
        width,
        height,
        image::ExtendedColorType::La8,
    )?;
    Ok(png)
}
