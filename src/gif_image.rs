//! A bounded first GIF frame at its offset on the logical canvas.
use crate::{Result, comic};
use image::{GrayImage, ImageEncoder, Pixel};
use std::{io::Cursor, num::NonZeroU64};

#[cfg(feature = "kindle")]
const MAX_FRAMES: usize = 128;
#[cfg(feature = "kindle")]
const MAX_FRAME_PIXELS: u64 = 32 * 1024 * 1024;

/// A bounded, memory-backed animated GIF. Later frames are composited on
/// demand so no collection of decoded page images is retained.
#[cfg(feature = "kindle")]
pub(crate) struct Sequence {
    data: Vec<u8>,
    width: u32,
    height: u32,
    frames: usize,
}

#[cfg(feature = "kindle")]
impl Sequence {
    pub(crate) fn open(data: Vec<u8>) -> Result<Self> {
        let (width, height) = dimensions(&data)?;
        let mut options = gif::DecodeOptions::new();
        options.set_memory_limit(gif::MemoryLimit::Bytes(
            NonZeroU64::new(comic::MAX_IMAGE_BYTES).unwrap(),
        ));
        options.check_frame_consistency(true);
        options.skip_frame_decoding(true);
        let mut decoder = options.read_info(Cursor::new(&data))?;
        let mut frames = 0usize;
        let canvas_pixels = u64::from(width) * u64::from(height);
        let mut pixels = canvas_pixels;
        while let Some(frame) = decoder.read_next_frame()? {
            let area = u64::from(frame.width) * u64::from(frame.height);
            if area == 0 || frames == MAX_FRAMES {
                return Err("This GIF has too many frames or an empty frame".into());
            }
            let restore = if frame.dispose == gif::DisposalMethod::Previous {
                canvas_pixels
            } else {
                0
            };
            pixels = pixels
                .checked_add(area)
                .and_then(|count| count.checked_add(restore))
                .filter(|&count| count <= MAX_FRAME_PIXELS)
                .ok_or("This GIF exceeds Reader's cumulative frame limit")?;
            frames += 1;
        }
        if frames == 0 {
            return Err("This GIF has no image frame".into());
        }
        if frames > 1 && canvas_pixels > 4 * 1024 * 1024 {
            return Err("This animated GIF exceeds Reader's canvas limit".into());
        }
        drop(decoder);
        Ok(Self {
            data,
            width,
            height,
            frames,
        })
    }

    pub(crate) fn page_count(&self) -> usize {
        self.frames
    }
    pub(crate) fn page_size(&self) -> (f32, f32) {
        (self.width as f32, self.height as f32)
    }

    pub(crate) fn image_page(&self, index: usize) -> Result<comic::ImagePage> {
        let gray = self.page(index)?;
        comic::ImagePage::from_grayscale(gray.width(), gray.height(), gray.into_raw())
    }

    pub(crate) fn page(&self, index: usize) -> Result<GrayImage> {
        use gif::DisposalMethod;
        if index >= self.frames {
            return Err("That GIF frame is unavailable".into());
        }
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::Indexed);
        options.set_memory_limit(gif::MemoryLimit::Bytes(
            NonZeroU64::new(comic::MAX_IMAGE_PIXELS).unwrap(),
        ));
        options.check_frame_consistency(true);
        let mut decoder = options.read_info(Cursor::new(&self.data))?;
        let global = decoder.global_palette().map(<[u8]>::to_vec);
        let (width, height) = (self.width as usize, self.height as usize);
        let mut canvas = vec![255u8; width * height];
        let mut previous: Option<(
            DisposalMethod,
            (usize, usize, usize, usize),
            Option<Vec<u8>>,
        )> = None;
        for frame_index in 0..=index {
            if let Some((disposal, (left, top, w, h), saved)) = previous.take() {
                match disposal {
                    DisposalMethod::Background => {
                        for row in top..top + h {
                            canvas[row * width + left..row * width + left + w].fill(255);
                        }
                    }
                    DisposalMethod::Previous => {
                        if let Some(saved) = saved {
                            canvas = saved;
                        }
                    }
                    _ => {}
                }
            }
            let frame = decoder
                .read_next_frame()?
                .ok_or("This GIF ends before the requested frame")?;
            let (left, top, w, h) = (
                usize::from(frame.left),
                usize::from(frame.top),
                usize::from(frame.width),
                usize::from(frame.height),
            );
            if w == 0
                || h == 0
                || left + w > width
                || top + h > height
                || frame.buffer.len() != w * h
            {
                return Err("This GIF contains an invalid frame".into());
            }
            let palette = frame
                .palette
                .as_deref()
                .or(global.as_deref())
                .ok_or("This GIF has no color palette")?;
            let colors: Vec<_> = palette
                .as_chunks::<3>()
                .0
                .iter()
                .map(|&rgb| image::Rgb(rgb).to_luma().0[0])
                .collect();
            let saved = (frame.dispose == DisposalMethod::Previous && frame_index < index)
                .then(|| canvas.clone());
            for (y, src) in frame.buffer.chunks_exact(w).enumerate() {
                let offset = (top + y) * width + left;
                for (&color, pixel) in src.iter().zip(&mut canvas[offset..offset + w]) {
                    if frame.transparent != Some(color) {
                        *pixel = *colors
                            .get(usize::from(color))
                            .ok_or("This GIF contains an invalid color index")?;
                    }
                }
            }
            previous = Some((frame.dispose, (left, top, w, h), saved));
        }
        GrayImage::from_raw(self.width, self.height, canvas)
            .ok_or_else(|| "GIF pixel dimensions are inconsistent".into())
    }
}

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
