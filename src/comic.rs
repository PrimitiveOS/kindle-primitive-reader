//! Native CBZ and standalone PNG/JPEG pages. Archive and image file handles
//! are short-lived so Reader can give up its USB storage lease between reads.

use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{BufReader, Cursor, Read};
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, metadata::Orientation};
use zip::{CompressionMethod, ZipArchive};

use crate::Result;
use crate::epub::check_zip_directory;
use crate::fixed::{MAX_VIEWPORT_PIXELS, MAX_VIEWPORT_SIDE, RenderRequest};

pub const MAX_COMIC_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_IMAGE_SIDE: u32 = 4096;
const MAX_IMAGE_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PROJECTED_SIDE: f32 = 32_768.0;
const MAX_MEMBER_NAME: usize = 512;

type Archive = ZipArchive<BufReader<File>>;

pub struct Comic {
    path: PathBuf,
    pages: Vec<Member>,
}

struct Member {
    index: usize,
    name: String,
    size: u64,
    crc: u32,
}

impl Comic {
    pub fn open(path: &Path) -> Result<Self> {
        let mut archive = open_archive(path)?;
        let mut pages = Vec::new();
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index)?;
            if !page_name(entry.name()) || !entry.is_file() {
                continue;
            }
            pages.push(Member {
                index,
                name: entry.name().to_owned(),
                size: entry.size(),
                crc: entry.crc32(),
            });
        }
        if pages.is_empty() {
            return Err("This CBZ contains no PNG or JPEG pages".into());
        }
        pages.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        Ok(Self {
            path: path.to_owned(),
            pages,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn page_name(&self, page: usize) -> Option<&str> {
        self.pages.get(page).map(|member| member.name.as_str())
    }

    pub fn decode_page(&self, page: usize) -> Result<ImagePage> {
        let member = self
            .pages
            .get(page)
            .ok_or("That page is outside this CBZ")?;
        let mut archive = open_archive(&self.path)?;
        {
            let header = archive.by_index_raw(member.index)?;
            if header.name() != member.name
                || header.size() != member.size
                || header.crc32() != member.crc
                || !header.is_file()
            {
                return Err("This CBZ changed after it was opened".into());
            }
            if header.encrypted() {
                return Err("This CBZ page is password-protected".into());
            }
            if !matches!(
                header.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            ) {
                return Err("This CBZ page uses an unsupported compression method".into());
            }
        }
        let bytes = read_image(archive.by_index(member.index)?, member.size)?;
        ImagePage::decode(&bytes)
    }
}

fn open_archive(path: &Path) -> Result<Archive> {
    let (file, size) = open_regular(path, MAX_COMIC_BYTES)?;
    let mut input = BufReader::new(file);
    let entry_count = check_zip_directory(&mut input, size)
        .map_err(|_| "This CBZ has an invalid or oversized ZIP directory")?;
    let archive = ZipArchive::new(input)?;
    if archive.len() != entry_count {
        return Err("This CBZ has duplicate or inconsistent entries".into());
    }
    Ok(archive)
}

fn page_name(name: &str) -> bool {
    if name.len() > MAX_MEMBER_NAME
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || name
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part == "__MACOSX")
    {
        return false;
    }
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        extension.eq_ignore_ascii_case("png")
            || extension.eq_ignore_ascii_case("jpg")
            || extension.eq_ignore_ascii_case("jpeg")
    })
}

/// Compare digit groups by numeric value without converting them to integers.
/// Break equal natural order by the whole name so the order is stable.
fn natural_cmp(left: &str, right: &str) -> Ordering {
    let (mut a, mut b) = (left.as_bytes(), right.as_bytes());
    while let (Some(&l), Some(&r)) = (a.first(), b.first()) {
        let order = if l.is_ascii_digit() && r.is_ascii_digit() {
            let left_end = a
                .iter()
                .position(|byte| !byte.is_ascii_digit())
                .unwrap_or(a.len());
            let right_end = b
                .iter()
                .position(|byte| !byte.is_ascii_digit())
                .unwrap_or(b.len());
            let left_digits = significant_digits(&a[..left_end]);
            let right_digits = significant_digits(&b[..right_end]);
            a = &a[left_end..];
            b = &b[right_end..];
            left_digits
                .len()
                .cmp(&right_digits.len())
                .then_with(|| left_digits.cmp(right_digits))
        } else {
            a = &a[1..];
            b = &b[1..];
            l.to_ascii_lowercase().cmp(&r.to_ascii_lowercase())
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len()).then_with(|| left.cmp(right))
}

fn significant_digits(digits: &[u8]) -> &[u8] {
    &digits[digits
        .iter()
        .position(|&digit| digit != b'0')
        .unwrap_or(digits.len())..]
}

pub struct ImagePage {
    width: u32,
    height: u32,
    gray: Vec<u8>,
}

impl ImagePage {
    pub(crate) fn from_grayscale(width: u32, height: u32, gray: Vec<u8>) -> Result<Self> {
        if !(1..=MAX_IMAGE_SIDE).contains(&width)
            || !(1..=MAX_IMAGE_SIDE).contains(&height)
            || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
            || gray.len() as u64 != u64::from(width) * u64::from(height)
        {
            return Err("The grayscale page exceeds Reader's image limits".into());
        }
        Ok(Self {
            width,
            height,
            gray,
        })
    }

    pub fn open(path: &Path) -> Result<Self> {
        let (file, size) = open_regular(path, MAX_IMAGE_BYTES)?;
        let bytes = read_image(file, size)?;
        Self::decode(&bytes)
    }

    pub fn page_size(&self) -> (f32, f32) {
        (self.width as f32, self.height as f32)
    }

    pub fn fit(&self, width: u32, height: u32, turns: u8) -> Result<Vec<u8>> {
        let (page_width, page_height) = self.rotated_size(turns);
        let scale = (width as f32 / page_width).min(height as f32 / page_height);
        self.render(RenderRequest {
            width,
            height,
            scale,
            x: (width as f32 - page_width * scale).max(0.0) / 2.0,
            y: (height as f32 - page_height * scale).max(0.0) / 2.0,
            turns,
        })
    }

    /// Draw an aspect-preserving view into a bounded grayscale viewport.
    /// Only visible destination pixels are sampled, including when zoomed.
    pub fn render(&self, request: RenderRequest) -> Result<Vec<u8>> {
        let count = request
            .width
            .checked_mul(request.height)
            .ok_or("This image view is too large")?;
        let (page_width, page_height) = self.rotated_size(request.turns);
        if request.width == 0
            || request.height == 0
            || count > MAX_VIEWPORT_PIXELS
            || request.width > MAX_VIEWPORT_SIDE
            || request.height > MAX_VIEWPORT_SIDE
            || !request.scale.is_finite()
            || request.scale <= 0.0
            || page_width.max(page_height) * request.scale > MAX_PROJECTED_SIDE
            || !request.x.is_finite()
            || !request.y.is_finite()
            || request.x.abs().max(request.y.abs()) > MAX_PROJECTED_SIDE
        {
            return Err("This image view exceeds Reader's viewport limits".into());
        }

        let mut frame = vec![255; count as usize];
        let turns = request.turns % 4;
        let (w, h) = ((self.width - 1) as f32, (self.height - 1) as f32);
        let scale = request.scale;
        let start_x = first_pixel(request.x, request.width);
        let end_x = first_pixel(request.x + page_width * scale, request.width);
        let start_y = first_pixel(request.y, request.height);
        let end_y = first_pixel(request.y + page_height * scale, request.height);
        let stride = request.width as usize;
        for y in start_y..end_y {
            let sy = (y as f32 + 0.5 - request.y) / scale - 0.5;
            let target = &mut frame[y * stride + start_x..y * stride + end_x];
            for (x, pixel) in (start_x..end_x).zip(target) {
                let sx = (x as f32 + 0.5 - request.x) / scale - 0.5;
                let (sx, sy) = match turns {
                    0 => (sx, sy),
                    1 => (sy, h - sx),
                    2 => (w - sx, h - sy),
                    _ => (w - sy, sx),
                };
                *pixel = self.sample(sx, sy);
            }
        }
        Ok(frame)
    }

    fn rotated_size(&self, turns: u8) -> (f32, f32) {
        if turns.is_multiple_of(2) {
            self.page_size()
        } else {
            (self.height as f32, self.width as f32)
        }
    }

    fn sample(&self, x: f32, y: f32) -> u8 {
        let x = x.clamp(0.0, (self.width - 1) as f32);
        let y = y.clamp(0.0, (self.height - 1) as f32);
        let (x0, y0) = (x as usize, y as usize);
        let stride = self.width as usize;
        let x1 = (x0 + 1).min(stride - 1);
        let y1 = (y0 + 1).min(self.height as usize - 1);
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let a = f32::from(self.gray[y0 * stride + x0]);
        let b = f32::from(self.gray[y0 * stride + x1]);
        let c = f32::from(self.gray[y1 * stride + x0]);
        let d = f32::from(self.gray[y1 * stride + x1]);
        let top = a + (b - a) * fx;
        let bottom = c + (d - c) * fx;
        (top + (bottom - top) * fy + 0.5) as u8
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
            return Err("Reader supports PNG and JPEG image pages".into());
        }
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_SIDE);
        limits.max_image_height = Some(MAX_IMAGE_SIDE);
        limits.max_alloc = Some(MAX_DECODE_BYTES);
        reader.limits(limits);
        let mut decoder = reader.into_decoder()?;
        let (width, height) = decoder.dimensions();
        if width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
            || decoder.total_bytes() > MAX_DECODE_BYTES
        {
            return Err("This image exceeds Reader's decoded image limit".into());
        }
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let decoded = DynamicImage::from_decoder(decoder)?;
        let mut gray = grayscale(decoded);
        if orientation != Orientation::NoTransforms {
            let mut rotated = DynamicImage::ImageLuma8(gray);
            rotated.apply_orientation(orientation);
            gray = rotated.into_luma8();
        }
        Ok(Self {
            width: gray.width(),
            height: gray.height(),
            gray: gray.into_raw(),
        })
    }
}

fn grayscale(decoded: DynamicImage) -> image::GrayImage {
    if let DynamicImage::ImageLuma8(image) = decoded {
        return image;
    }
    let gray_alpha = decoded.into_luma_alpha8();
    image::GrayImage::from_fn(gray_alpha.width(), gray_alpha.height(), |x, y| {
        let [gray, alpha] = gray_alpha.get_pixel(x, y).0.map(u16::from);
        image::Luma([((gray * alpha + 255 * (255 - alpha) + 127) / 255) as u8])
    })
}

fn first_pixel(edge: f32, side: u32) -> usize {
    (edge - 0.5).ceil().clamp(0.0, side as f32) as usize
}

fn open_regular(path: &Path, max_bytes: u64) -> Result<(File, u64)> {
    if !fs::metadata(path)?.is_file() {
        return Err("Reader can only open regular image or CBZ files".into());
    }
    let file = File::open(path)?;
    let info = file.metadata()?;
    if !info.is_file() || info.len() > max_bytes {
        return Err("This image or CBZ exceeds Reader's file size limit".into());
    }
    Ok((file, info.len()))
}

fn read_image(reader: impl Read, size: u64) -> Result<Vec<u8>> {
    if !(1..=MAX_IMAGE_BYTES).contains(&size) {
        return Err("This image page exceeds Reader's input size limit or is empty".into());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    reader.take(size + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size {
        return Err("This image page's size changed or its data is incomplete".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use image::{GrayImage, Luma, Rgba, RgbaImage};
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(ext: &str) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self(std::env::temp_dir().join(format!(
                "reader-comic-{}-{}.{ext}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn png(image: DynamicImage) -> Vec<u8> {
        let mut data = Cursor::new(Vec::new());
        image.write_to(&mut data, ImageFormat::Png).unwrap();
        data.into_inner()
    }

    fn comic(path: &Path, pages: &[(&str, &[u8])]) {
        let mut writer = ZipWriter::new(File::create(path).unwrap());
        for &(name, data) in pages {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn sorts_numbered_pages_and_skips_unsafe_or_nonimage_entries() {
        let fixture = Fixture::new("cbz");
        let black = png(GrayImage::from_pixel(2, 2, Luma([0])).into());
        let white = png(GrayImage::from_pixel(2, 2, Luma([255])).into());
        comic(
            &fixture.0,
            &[
                ("pages/10.png", &white),
                ("README.txt", b"Fixture"),
                ("pages/2.png", &white),
                ("__MACOSX/0.png", &white),
                ("pages/1.PNG", &black),
                ("../outside.png", &white),
            ],
        );
        let document = Comic::open(&fixture.0).unwrap();
        assert_eq!(document.page_count(), 3);
        assert_eq!(document.page_name(0), Some("pages/1.PNG"));
        assert_eq!(document.page_name(1), Some("pages/2.png"));
        assert_eq!(document.page_name(2), Some("pages/10.png"));
        assert_eq!(
            document.decode_page(0).unwrap().fit(4, 4, 0).unwrap(),
            vec![0; 16]
        );
        assert!(document.decode_page(3).is_err());
        comic(
            &fixture.0,
            &[
                ("pages/10.png", &white),
                ("pages/2.png", &white),
                ("pages/1.PNG", &white),
            ],
        );
        assert!(document.decode_page(0).is_err());
    }

    #[test]
    fn renders_alpha_pan_and_quarter_turns_in_a_bounded_view() {
        let fixture = Fixture::new("png");
        let image = RgbaImage::from_fn(2, 2, |x, y| {
            if x == 0 && y == 0 {
                Rgba([0, 0, 0, 255])
            } else if x == 1 && y == 0 {
                Rgba([30, 30, 30, 0])
            } else {
                Rgba([0, 0, 0, 128])
            }
        });
        fs::write(&fixture.0, png(image.into())).unwrap();
        let page = ImagePage::open(&fixture.0).unwrap();
        assert_eq!(page.page_size(), (2.0, 2.0));
        assert_eq!(page.fit(2, 2, 0).unwrap(), [0, 255, 127, 127]);
        assert_eq!(page.fit(2, 2, 1).unwrap(), [127, 0, 127, 255]);
        let view = RenderRequest {
            width: 2,
            height: 2,
            scale: 1.0,
            x: 1.0,
            y: 0.0,
            turns: 0,
        };
        assert_eq!(page.render(view).unwrap(), [255, 0, 255, 127]);
        assert!(
            page.render(RenderRequest {
                width: 4096,
                ..view
            })
            .is_err()
        );
    }

    #[test]
    fn opens_jpeg_with_exif_orientation_and_rejects_oversized_images() {
        let fixture = Fixture::new("jpg");
        let pixels = GrayImage::from_pixel(3, 2, Luma([40]));
        let mut jpeg = Cursor::new(Vec::new());
        DynamicImage::ImageLuma8(pixels)
            .write_to(&mut jpeg, ImageFormat::Jpeg)
            .unwrap();
        let mut bytes = jpeg.into_inner();
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut segment = vec![0xff, 0xe1];
        segment.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        segment.extend_from_slice(exif);
        bytes.splice(2..2, segment);
        fs::write(&fixture.0, bytes).unwrap();
        assert_eq!(ImagePage::open(&fixture.0).unwrap().page_size(), (2.0, 3.0));

        let fixture = Fixture::new("png");
        let wide = GrayImage::from_pixel(MAX_IMAGE_SIDE + 1, 1, Luma([0]));
        fs::write(&fixture.0, png(wide.into())).unwrap();
        assert!(ImagePage::open(&fixture.0).is_err());
    }
}
