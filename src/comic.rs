//! Native CBZ, USTAR CBT, bounded CBR, and standalone raster pages. File handles
//! are short-lived so Reader can give up its USB storage lease between reads.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, Pixel, metadata::Orientation,
};
use zip::{CompressionMethod, ZipArchive};

use crate::Result;
use crate::cbr::Cbr;
use crate::epub::{check_zip_directory, is_archive_path};
use crate::fixed::{MAX_VIEWPORT_PIXELS, MAX_VIEWPORT_SIDE, RenderRequest};

pub const MAX_COMIC_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_WEBP_BYTES: u64 = 4 * 1024 * 1024;
const MAX_COMIC_MEMBERS: usize = 8192;
pub(crate) const MAX_IMAGE_SIDE: u32 = 4096;
pub(crate) const MAX_IMAGE_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_WEBP_PIXELS: u64 = 4 * 1024 * 1024;
const MAX_PROJECTED_SIDE: f32 = 32_768.0;
const MAX_MEMBER_NAME: usize = 512;
const TAR_BLOCK_BYTES: usize = 512;
const MAX_TAR_TRAILER_BYTES: u64 = 1024 * 1024;

type Archive = ZipArchive<BufReader<File>>;

pub struct Comic {
    source: ComicSource,
}

enum ComicSource {
    ZipOrTar { path: PathBuf, pages: Vec<Member> },
    Rar(Cbr),
}

struct Member {
    name: String,
    size: u64,
    location: MemberLocation,
}

#[derive(Clone, Copy)]
enum MemberLocation {
    Zip {
        index: usize,
        crc: u32,
    },
    Tar {
        header: u64,
        length: u64,
        modified: Option<SystemTime>,
    },
}

impl Comic {
    pub fn open(path: &Path) -> Result<Self> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("cbr") {
            return Ok(Self {
                source: ComicSource::Rar(Cbr::open(path)?),
            });
        }
        let mut pages = if extension.eq_ignore_ascii_case("cbt") {
            tar_pages(path)?
        } else {
            zip_pages(path)?
        };
        if pages.is_empty() {
            return Err("This comic archive contains no supported image pages".into());
        }
        pages.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        Ok(Self {
            source: ComicSource::ZipOrTar {
                path: path.to_owned(),
                pages,
            },
        })
    }

    pub fn page_count(&self) -> usize {
        match &self.source {
            ComicSource::ZipOrTar { pages, .. } => pages.len(),
            ComicSource::Rar(comic) => comic.page_count(),
        }
    }

    pub fn page_name(&self, page: usize) -> Option<&str> {
        match &self.source {
            ComicSource::ZipOrTar { pages, .. } => {
                pages.get(page).map(|member| member.name.as_str())
            }
            ComicSource::Rar(comic) => comic.page_name(page),
        }
    }

    pub fn decode_page(&self, page: usize) -> Result<ImagePage> {
        let (path, pages) = match &self.source {
            ComicSource::ZipOrTar { path, pages } => (path, pages),
            ComicSource::Rar(comic) => {
                return ImagePage::decode(&comic.read_page(page)?);
            }
        };
        let member = pages
            .get(page)
            .ok_or("That page is outside this comic archive")?;
        let bytes = match member.location {
            MemberLocation::Zip { index, crc } => zip_image(path, member, index, crc)?,
            MemberLocation::Tar {
                header,
                length,
                modified,
            } => tar_image(path, member, header, length, modified)?,
        };
        ImagePage::decode(&bytes)
    }
}

fn zip_pages(path: &Path) -> Result<Vec<Member>> {
    let mut archive = open_archive(path)?;
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        if page_name(entry.name()) && entry.is_file() {
            pages.push(Member {
                name: entry.name().to_owned(),
                size: entry.size(),
                location: MemberLocation::Zip {
                    index,
                    crc: entry.crc32(),
                },
            });
        }
    }
    Ok(pages)
}

fn zip_image(path: &Path, member: &Member, index: usize, crc: u32) -> Result<Vec<u8>> {
    if member.size > raster_input_limit(Path::new(&member.name)) {
        return Err("This image page exceeds Reader's input size limit or is empty".into());
    }
    let mut archive = open_archive(path)?;
    {
        let header = archive.by_index_raw(index)?;
        if header.name() != member.name
            || header.size() != member.size
            || header.crc32() != crc
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
    read_image(archive.by_index(index)?, member.size)
}

fn open_archive(path: &Path) -> Result<Archive> {
    let (file, size) = open_regular(path, MAX_COMIC_BYTES)?;
    let mut input = BufReader::new(file);
    let entry_count = check_zip_directory(&mut input, size)
        .map_err(|_| "This CBZ has an invalid or oversized ZIP directory")?;
    if entry_count > MAX_COMIC_MEMBERS {
        return Err("This CBZ has too many entries".into());
    }
    let archive = ZipArchive::new(input)?;
    if archive.len() != entry_count {
        return Err("This CBZ has duplicate or inconsistent entries".into());
    }
    Ok(archive)
}

struct TarHeader {
    name: String,
    size: u64,
    directory: bool,
}

fn tar_pages(path: &Path) -> Result<Vec<Member>> {
    let (file, length) = open_regular(path, MAX_COMIC_BYTES)?;
    let block_bytes = TAR_BLOCK_BYTES as u64;
    if length < block_bytes * 2 || !length.is_multiple_of(block_bytes) {
        return Err("The CBT length is invalid for a USTAR archive".into());
    }
    let modified = file.metadata()?.modified().ok();
    let mut input = BufReader::new(file);
    let mut offset = 0;
    let mut names = HashSet::new();
    let mut pages = Vec::new();
    let mut block = [0; TAR_BLOCK_BYTES];
    for count in 0..=MAX_COMIC_MEMBERS {
        input.read_exact(&mut block)?;
        if block.iter().all(|&byte| byte == 0) {
            check_tar_trailer(&mut input, length - offset - block_bytes)?;
            return Ok(pages);
        }
        if count == MAX_COMIC_MEMBERS {
            return Err("The CBT has too many entries".into());
        }
        let header = parse_tar_header(&block)?;
        if !names.insert(header.name.clone()) {
            return Err("The CBT has duplicate entry names".into());
        }
        let next = tar_next_header(offset, header.size, length)?;
        if !header.directory && page_name(&header.name) {
            if !(1..=raster_input_limit(Path::new(&header.name))).contains(&header.size) {
                return Err("A CBT page is empty or exceeds the image size limit".into());
            }
            pages.push(Member {
                name: header.name,
                size: header.size,
                location: MemberLocation::Tar {
                    header: offset,
                    length,
                    modified,
                },
            });
        }
        input.seek_relative(i64::try_from(next - offset - block_bytes)?)?;
        offset = next;
    }
    Err("The CBT has no archive trailer".into())
}

fn tar_image(
    path: &Path,
    member: &Member,
    offset: u64,
    archive_length: u64,
    modified: Option<SystemTime>,
) -> Result<Vec<u8>> {
    let (mut file, length) = open_regular(path, MAX_COMIC_BYTES)?;
    if length != archive_length
        || modified.is_some() && file.metadata()?.modified().ok() != modified
    {
        return Err("This CBT changed after it was opened".into());
    }
    tar_next_header(offset, member.size, length)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut block = [0; TAR_BLOCK_BYTES];
    file.read_exact(&mut block)?;
    let header = parse_tar_header(&block)?;
    if header.directory || header.name != member.name || header.size != member.size {
        return Err("This CBT changed after it was opened".into());
    }
    read_image(file.take(member.size), member.size)
}

fn tar_next_header(offset: u64, size: u64, length: u64) -> Result<u64> {
    let block_bytes = TAR_BLOCK_BYTES as u64;
    size.checked_next_multiple_of(block_bytes)
        .and_then(|padded| offset.checked_add(block_bytes)?.checked_add(padded))
        .filter(|&next| next <= length.saturating_sub(2 * block_bytes))
        .ok_or_else(|| "A CBT entry exceeds the archive length".into())
}

fn check_tar_trailer(input: &mut impl Read, mut remaining: u64) -> Result<()> {
    if !(TAR_BLOCK_BYTES as u64..=MAX_TAR_TRAILER_BYTES).contains(&remaining) {
        return Err("The CBT has an invalid USTAR trailer length".into());
    }
    let mut buffer = [0; 4096];
    while remaining > 0 {
        let count = remaining.min(buffer.len() as u64) as usize;
        input.read_exact(&mut buffer[..count])?;
        if buffer[..count].iter().any(|&byte| byte != 0) {
            return Err("The CBT has data after its archive trailer".into());
        }
        remaining -= count as u64;
    }
    Ok(())
}

fn parse_tar_header(block: &[u8; TAR_BLOCK_BYTES]) -> Result<TarHeader> {
    if &block[257..265] != b"ustar\0\x30\x30" {
        return Err("The CBT uses a TAR format outside Reader's USTAR support".into());
    }
    let checksum = block[..148]
        .iter()
        .chain(&block[156..])
        .map(|&byte| u64::from(byte))
        .sum::<u64>()
        + (u64::from(b' ') * 8);
    if tar_octal(&block[148..156]) != Some(checksum) {
        return Err("A CBT entry has an invalid USTAR checksum".into());
    }
    let size = tar_octal(&block[124..136]).ok_or("A CBT entry has an invalid USTAR size")?;
    let directory = match block[156] {
        b'0' | 0 => false,
        b'5' if size == 0 => true,
        _ => return Err("The CBT has a link, special file, or unsupported TAR extension".into()),
    };
    if block[157..257].iter().any(|&byte| byte != 0) {
        return Err("A CBT entry has a link name".into());
    }
    let prefix = tar_string(&block[345..500])?;
    let name = tar_string(&block[..100])?;
    if name.is_empty() {
        return Err("A CBT entry has an empty USTAR name".into());
    }
    let joined = if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    };
    let name = joined.strip_prefix("./").unwrap_or(&joined);
    let name = if directory {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if directory && (joined == "./" || joined == ".") {
        return Ok(TarHeader {
            name: String::from("."),
            size,
            directory,
        });
    }
    if !is_archive_path(name) {
        return Err("The CBT has an unsafe entry path".into());
    }
    Ok(TarHeader {
        name: name.to_owned(),
        size,
        directory,
    })
}

fn tar_string(field: &[u8]) -> Result<&str> {
    let end = field
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(field.len());
    if field[end..].iter().any(|&byte| byte != 0) {
        return Err("A CBT entry has invalid USTAR name padding".into());
    }
    Ok(std::str::from_utf8(&field[..end])?)
}

fn tar_octal(field: &[u8]) -> Option<u64> {
    let start = field.iter().position(|&byte| byte != b' ')?;
    let field = &field[start..];
    let end = field
        .iter()
        .take_while(|&&byte| matches!(byte, b'0'..=b'7'))
        .count();
    if end == 0 || field[end..].iter().any(|&byte| !matches!(byte, 0 | b' ')) {
        return None;
    }
    u64::from_str_radix(std::str::from_utf8(&field[..end]).ok()?, 8).ok()
}

pub(crate) fn page_name(name: &str) -> bool {
    if name.len() > MAX_MEMBER_NAME
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || name
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part == "__MACOSX")
    {
        return false;
    }
    name.rsplit_once('.')
        .is_some_and(|(_, extension)| crate::library::DocumentFormat::raster_for_extension(extension).is_some())
}

/// Compare digit groups by numeric value without converting them to integers.
/// Break equal natural order by the whole name so the order is stable.
pub(crate) fn natural_cmp(left: &str, right: &str) -> Ordering {
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
        if !image_dimensions_valid(width, height)
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
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            let (file, length) = open_regular(path, crate::svg_image::MAX_SVG_BYTES as u64)?;
            let bytes = read_image(file, length)?;
            let image = crate::svg_image::rasterize(
                &bytes,
                crate::svg_image::MAX_SVG_SIDE,
                crate::svg_image::MAX_SVG_SIDE,
                crate::svg_image::MAX_SVG_PIXELS,
            )?;
            return Self::from_grayscale(image.width(), image.height(), image.into_raw());
        }
        let (file, size) = open_regular(path, raster_input_limit(path))?;
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
        if crate::jpeg2000::has_signature(bytes) {
            let gray = crate::jpeg2000::decode(bytes)?;
            return Self::from_grayscale(gray.width(), gray.height(), gray.into_raw());
        }
        let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        match reader.format() {
            Some(ImageFormat::Gif) => return Self::decode_gif(bytes),
            Some(ImageFormat::WebP) => return Self::decode_webp(bytes),
            Some(ImageFormat::Tiff) => {
                let image = crate::tiff_image::decode(bytes)?;
                return Self::from_grayscale(image.width(), image.height(), image.into_raw());
            }
            Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Bmp | ImageFormat::Pnm) => (),
            _ => return Err("This image page uses an unsupported format".into()),
        }
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_SIDE);
        limits.max_image_height = Some(MAX_IMAGE_SIDE);
        limits.max_alloc = Some(MAX_DECODE_BYTES);
        reader.limits(limits);
        let mut decoder = reader.into_decoder()?;
        let (width, height) = decoder.dimensions();
        if !image_dimensions_valid(width, height) || decoder.total_bytes() > MAX_DECODE_BYTES {
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

    fn decode_webp(bytes: &[u8]) -> Result<Self> {
        let (width, height) = simple_webp_dimensions(bytes)?;
        let rgb_bytes = u64::from(width) * u64::from(height) * 3;
        let rgb = std::panic::catch_unwind(|| -> Result<Vec<u8>> {
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))?;
            if decoder.dimensions() != (width, height)
                || decoder.color_type() != image::ColorType::Rgb8
                || decoder.total_bytes() != rgb_bytes
                || decoder.has_animation()
            {
                return Err("The WebP decoder reports an unsupported image format".into());
            }
            let mut pixels = vec![0; rgb_bytes as usize];
            decoder.read_image(&mut pixels)?;
            Ok(pixels)
        })
        .map_err(|_| "The WebP frame could not be decoded")??;
        let gray = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&pixel| image::Rgb(pixel).to_luma().0[0])
            .collect();
        Self::from_grayscale(width, height, gray)
    }

    pub(crate) fn gif_as_png(bytes: &[u8]) -> Result<Vec<u8>> {
        use image::ImageEncoder;
        let image = Self::decode_gif(bytes)?;
        let mut output = Vec::new();
        image::codecs::png::PngEncoder::new(&mut output).write_image(
            &image.gray, image.width, image.height, image::ExtendedColorType::L8,
        )?;
        Ok(output)
    }

    pub(crate) fn decode_gif(bytes: &[u8]) -> Result<Self> {
        let gray = crate::gif_image::gray(bytes)?;
        Self::from_grayscale(gray.width(), gray.height(), gray.into_raw())
    }
}

pub(crate) fn raster_input_limit(path: &Path) -> u64 {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(crate::library::DocumentFormat::raster_for_extension)
        .map_or(MAX_IMAGE_BYTES, crate::library::DocumentFormat::max_bytes)
}

// Simple lossy WebP contains only RIFF/WEBP and one VP8 key-frame chunk.
// Check the raw VP8 dimensions before the decoder allocates its YUV planes.
fn simple_webp_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    const HEADER_BYTES: usize = 20;
    const FRAME_HEADER_BYTES: usize = 10;
    if bytes.len() as u64 > MAX_WEBP_BYTES {
        return Err("The WebP exceeds Reader's input size limit".into());
    }
    let header = bytes
        .get(..HEADER_BYTES)
        .ok_or("The WebP header is incomplete")?;
    if !header.starts_with(b"RIFF") || &header[8..16] != b"WEBPVP8 " {
        return Err("Reader supports simple lossy WebP with one VP8 image".into());
    }
    let riff_size = u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap()));
    let chunk_size = u64::from(u32::from_le_bytes(header[16..20].try_into().unwrap()));
    let padding = chunk_size % 2;
    if riff_size + 8 != bytes.len() as u64
        || HEADER_BYTES as u64 + chunk_size + padding != bytes.len() as u64
        || padding != 0 && bytes.last() != Some(&0)
        || chunk_size < FRAME_HEADER_BYTES as u64
    {
        return Err("Reader supports one complete VP8 chunk in a simple lossy WebP".into());
    }
    let frame = &bytes[HEADER_BYTES..HEADER_BYTES + FRAME_HEADER_BYTES];
    let tag = u32::from_le_bytes([frame[0], frame[1], frame[2], 0]);
    let version = (tag >> 1) & 7;
    let first_partition = u64::from(tag >> 5);
    if tag & 1 != 0
        || tag & 0x10 == 0
        || version > 3
        || frame[3..6] != [0x9d, 0x01, 0x2a]
        || first_partition == 0
        || first_partition > chunk_size - FRAME_HEADER_BYTES as u64
    {
        return Err("The WebP has an invalid or incomplete VP8 key frame".into());
    }
    let width = u32::from(u16::from_le_bytes([frame[6], frame[7]]) & 0x3fff);
    let height = u32::from(u16::from_le_bytes([frame[8], frame[9]]) & 0x3fff);
    let pixels = u64::from(width) * u64::from(height);
    if !image_dimensions_valid(width, height)
        || pixels > MAX_WEBP_PIXELS
        || pixels * 3 > MAX_DECODE_BYTES
    {
        return Err("The WebP exceeds Reader's decoded image limit".into());
    }
    Ok((width, height))
}

pub(crate) fn image_dimensions_valid(width: u32, height: u32) -> bool {
    (1..=MAX_IMAGE_SIDE).contains(&width)
        && (1..=MAX_IMAGE_SIDE).contains(&height)
        && u64::from(width) * u64::from(height) <= MAX_IMAGE_PIXELS
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
        return Err("Reader can only open regular image or comic files".into());
    }
    let file = File::open(path)?;
    let info = file.metadata()?;
    if !info.is_file() || info.len() > max_bytes {
        return Err("This image or comic exceeds Reader's file size limit".into());
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

    // cwebp 1.6.0, quality 90, encoded two 8×8 grayscale blocks from an original PPM.
    const SIMPLE_LOSSY_WEBP: &[u8] = &[
        0x52, 0x49, 0x46, 0x46, 0x28, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38,
        0x20, 0x1c, 0x00, 0x00, 0x00, 0x30, 0x01, 0x00, 0x9d, 0x01, 0x2a, 0x10, 0x00, 0x08, 0x00,
        0x00, 0xc0, 0x12, 0x25, 0xa4, 0x00, 0x03, 0x70, 0x00, 0xfe, 0xe7, 0xcf, 0xff, 0xff, 0xa6,
        0xea, 0x00, 0x00,
    ];

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

    #[test]
    fn opens_static_svg_files_with_the_image_viewport_limits() {
        let fixture = Fixture::new("SVG");
        fs::write(&fixture.0, br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="20" height="20" fill="black"/></svg>"#).unwrap();
        let image = ImagePage::open(&fixture.0).unwrap();
        assert_eq!(image.page_size(), (40.0, 20.0));
        let fitted = image.fit(80, 40, 0).unwrap();
        assert_eq!(fitted[80 * 20 + 5], 0);
        assert_eq!(fitted[80 * 20 + 75], 255);
        fs::write(&fixture.0, br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><image href="file:///tmp/picture.png"/></svg>"#).unwrap();
        assert!(ImagePage::open(&fixture.0).is_err());
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

    fn ustar_header(name: &str, prefix: &str, kind: u8, size: u64) -> [u8; TAR_BLOCK_BYTES] {
        let mut header = [0; TAR_BLOCK_BYTES];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        header[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[156] = kind;
        header[257..265].copy_from_slice(b"ustar\0\x30\x30");
        header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        header[148..156].fill(b' ');
        let checksum: u32 = header.iter().map(|&byte| u32::from(byte)).sum();
        header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
        header
    }

    fn tar_entry(archive: &mut Vec<u8>, name: &str, prefix: &str, kind: u8, bytes: &[u8]) {
        archive.extend(ustar_header(name, prefix, kind, bytes.len() as u64));
        archive.extend(bytes);
        archive.resize(archive.len().next_multiple_of(TAR_BLOCK_BYTES), 0);
    }

    fn tar_fixture(mut bytes: Vec<u8>) -> Fixture {
        bytes.resize(bytes.len() + 2 * TAR_BLOCK_BYTES, 0);
        let fixture = Fixture::new("CBT");
        fs::write(&fixture.0, bytes).unwrap();
        fixture
    }

    #[test]
    fn ustar_comic_indexes_natural_order_and_decodes_pages_by_offset() {
        let black = png(GrayImage::from_pixel(3, 2, Luma([0])).into());
        let white = png(GrayImage::from_pixel(3, 2, Luma([255])).into());
        let mut bytes = Vec::new();
        tar_entry(&mut bytes, "./", "", b'5', b"");
        tar_entry(&mut bytes, "./pages/", "", b'5', b"");
        tar_entry(&mut bytes, "10.png", "./pages", b'0', &white);
        tar_entry(&mut bytes, "./readme.txt", "", b'0', b"Comic fixture");
        tar_entry(&mut bytes, "./pages/2.PNG", "", 0, &black);
        let fixture = tar_fixture(bytes);
        let document = Comic::open(&fixture.0).unwrap();
        assert_eq!(document.page_count(), 2);
        assert_eq!(document.page_name(0), Some("pages/2.PNG"));
        assert_eq!(document.page_name(1), Some("pages/10.png"));
        assert_eq!(
            document.decode_page(1).unwrap().fit(3, 2, 0).unwrap(),
            [255; 6]
        );
        assert_eq!(
            document.decode_page(0).unwrap().fit(2, 3, 1).unwrap(),
            [0; 6]
        );
        assert!(document.decode_page(2).is_err());
        let mut changed = fs::OpenOptions::new()
            .append(true)
            .open(&fixture.0)
            .unwrap();
        changed.write_all(&[0; TAR_BLOCK_BYTES]).unwrap();
        assert!(document.decode_page(0).is_err());
    }

    #[test]
    fn ustar_comic_rejects_traversal_extensions_and_duplicate_paths() {
        for (name, prefix, kind) in [
            ("../outside.png", "", b'0'),
            ("absolute.png", "/outside", b'0'),
            ("PaxHeader", "", b'x'),
            ("LongName", "", b'L'),
            ("Sparse", "", b'S'),
            ("link.png", "", b'2'),
        ] {
            let mut bytes = Vec::new();
            tar_entry(&mut bytes, name, prefix, kind, b"fixture");
            assert!(Comic::open(&tar_fixture(bytes).0).is_err(), "{name}");
        }
        let mut bytes = Vec::new();
        tar_entry(&mut bytes, "page.pgm", "", b'0', b"P5\n1 1\n255\n\0");
        tar_entry(&mut bytes, "./page.pgm", "", b'0', b"P5\n1 1\n255\n\0");
        assert!(Comic::open(&tar_fixture(bytes).0).is_err());
    }

    #[test]
    fn ustar_comic_checks_header_size_checksum_entry_count_and_empty_trailer() {
        let image = b"P5\n1 1\n255\n\0";
        let mut bytes = Vec::new();
        tar_entry(&mut bytes, "page.pgm", "", b'0', image);
        let mut corrupt = bytes.clone();
        corrupt[0] = b'X';
        assert!(Comic::open(&tar_fixture(corrupt).0).is_err());
        let invalid_size = ustar_header("page.pgm", "", b'0', 0o77777777777);
        assert!(Comic::open(&tar_fixture(invalid_size.to_vec()).0).is_err());
        assert!(tar_next_header(0, u64::MAX, MAX_COMIC_BYTES).is_err());
        let mut unexpected_tail = bytes.clone();
        unexpected_tail.extend([0; TAR_BLOCK_BYTES]);
        unexpected_tail.extend(ustar_header("extra.pgm", "", b'0', image.len() as u64));
        assert!(Comic::open(&tar_fixture(unexpected_tail).0).is_err());
        for index in 0..MAX_COMIC_MEMBERS {
            tar_entry(&mut bytes, &format!("dir{index}"), "", b'5', b"");
        }
        assert!(Comic::open(&tar_fixture(bytes).0).is_err());
    }

    fn gif_bytes(width: u16, height: u16, frames: &[gif::Frame<'_>]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut encoder =
            gif::Encoder::new(&mut data, width, height, &[0, 0, 0, 255, 255, 255]).unwrap();
        for frame in frames {
            encoder.write_frame(frame).unwrap();
        }
        drop(encoder);
        data
    }

    #[test]
    fn simple_lossy_webp_renders_as_a_standalone_image_and_comic_page() {
        let fixture = Fixture::new("WEBP");
        fs::write(&fixture.0, SIMPLE_LOSSY_WEBP).unwrap();
        assert_eq!(
            crate::library::DocumentFormat::raster_for_extension("WebP"),
            Some(crate::library::DocumentFormat::WebP)
        );
        let page = ImagePage::open(&fixture.0).unwrap();
        assert_eq!(page.page_size(), (16.0, 8.0));
        let raster = page.fit(16, 8, 0).unwrap();
        assert!(raster[16 * 4 + 3] < 40);
        assert!(raster[16 * 4 + 12] > 190);

        let zip = Fixture::new("cbz");
        comic(&zip.0, &[("pages/1.WebP", SIMPLE_LOSSY_WEBP)]);
        let cbz = Comic::open(&zip.0).unwrap();
        assert_eq!(cbz.decode_page(0).unwrap().fit(16, 8, 0).unwrap(), raster);
        let mut bytes = Vec::new();
        tar_entry(&mut bytes, "pages/1.webp", "", b'0', SIMPLE_LOSSY_WEBP);
        let tar = tar_fixture(bytes);
        let cbt = Comic::open(&tar.0).unwrap();
        assert_eq!(cbt.decode_page(0).unwrap().fit(16, 8, 0).unwrap(), raster);
    }

    #[test]
    fn webp_preflight_rejects_extended_or_inconsistent_riff_chunks() {
        for chunk in [b"VP8X", b"VP8L", b"ALPH", b"ANIM"] {
            let mut unsupported = SIMPLE_LOSSY_WEBP.to_vec();
            unsupported[12..16].copy_from_slice(chunk);
            assert!(ImagePage::decode(&unsupported).is_err());
        }
        assert!(simple_webp_dimensions(&SIMPLE_LOSSY_WEBP[..SIMPLE_LOSSY_WEBP.len() - 1]).is_err());
        let mut additional = SIMPLE_LOSSY_WEBP.to_vec();
        additional[4..8].copy_from_slice(&48_u32.to_le_bytes());
        additional.extend(b"EXIF\0\0\0\0");
        assert!(simple_webp_dimensions(&additional).is_err());

        let mut odd = SIMPLE_LOSSY_WEBP.to_vec();
        odd[4..8].copy_from_slice(&42_u32.to_le_bytes());
        odd[16..20].copy_from_slice(&29_u32.to_le_bytes());
        odd.extend([0, 0]);
        assert_eq!(simple_webp_dimensions(&odd).unwrap(), (16, 8));
        odd[49] = 1;
        assert!(simple_webp_dimensions(&odd).is_err());
    }

    #[test]
    fn webp_preflight_checks_keyframe_partitions_dimensions_and_file_cap() {
        for (width, height) in [(0_u16, 16_u16), (4097, 16), (4096, 2048)] {
            let mut oversized = SIMPLE_LOSSY_WEBP.to_vec();
            oversized[26..28].copy_from_slice(&width.to_le_bytes());
            oversized[28..30].copy_from_slice(&height.to_le_bytes());
            assert!(simple_webp_dimensions(&oversized).is_err());
        }
        let mut malformed = SIMPLE_LOSSY_WEBP.to_vec();
        malformed[20] |= 1;
        assert!(simple_webp_dimensions(&malformed).is_err());
        let tag = (1000_u32 << 5) | 0x10;
        malformed[20..23].copy_from_slice(&tag.to_le_bytes()[..3]);
        assert!(simple_webp_dimensions(&malformed).is_err());
        malformed[20..23].copy_from_slice(&SIMPLE_LOSSY_WEBP[20..23]);
        malformed[23] ^= 1;
        assert!(simple_webp_dimensions(&malformed).is_err());

        let fixture = Fixture::new("webp");
        File::create(&fixture.0)
            .unwrap()
            .set_len(MAX_WEBP_BYTES + 1)
            .unwrap();
        assert!(
            ImagePage::open(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("file size limit")
        );
    }

    #[test]
    fn uses_only_the_gif_first_frame_with_local_palette_and_transparency() {
        let mut first =
            gif::Frame::from_palette_pixels(2, 1, [1, 0], [255, 0, 0, 0, 0, 0], Some(0));
        first.left = 1;
        first.top = 1;
        let second = gif::Frame::from_indexed_pixels(3, 2, [0; 6], None);
        let bytes = gif_bytes(3, 2, &[first, second]);
        let fixture = Fixture::new("gif");
        fs::write(&fixture.0, bytes).unwrap();
        let page = ImagePage::open(&fixture.0).unwrap();
        assert_eq!(page.page_size(), (3.0, 2.0));
        assert_eq!(page.fit(3, 2, 0).unwrap(), [255, 255, 255, 255, 0, 255]);
    }

    #[test]
    fn rejects_gif_oversized_canvas_and_frames_outside_it() {
        let pixel = gif::Frame::from_indexed_pixels(1, 1, [0], None);
        for (width, height) in [(4097, 1), (3000, 3000), (0, 1)] {
            assert!(
                ImagePage::decode(&gif_bytes(width, height, std::slice::from_ref(&pixel))).is_err()
            );
        }
        let mut outside = pixel;
        outside.left = 2;
        assert!(ImagePage::decode(&gif_bytes(2, 1, &[outside])).is_err());
        assert!(ImagePage::decode(&gif_bytes(2, 1, &[])).is_err());
    }

    #[test]
    fn decodes_bmp_and_portable_anymaps_standalone_and_in_cbz() {
        let mut bmp = Cursor::new(Vec::new());
        DynamicImage::ImageLuma8(GrayImage::from_raw(2, 1, vec![0, 255]).unwrap())
            .write_to(&mut bmp, ImageFormat::Bmp)
            .unwrap();
        let first = gif::Frame::from_indexed_pixels(2, 1, [0, 1], None);
        let gif = gif_bytes(2, 1, &[first]);
        let pages: &[(&str, &[u8])] = &[
            ("1.BMP", bmp.get_ref()),
            ("2.PBM", b"P1\n2 1\n1 0\n"),
            ("3.PGM", b"P5\n2 1\n255\n\x00\xff"),
            ("4.PPM", b"P3\n2 1\n255\n0 0 0 255 255 255\n"),
            ("5.PAM", b"P7\nWIDTH 2\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n\x00\x00\x00\xff\x00\x00\x00\x00"),
            ("6.PNM", b"P2\n2 1\n255\n0 255\n"),
            ("7.GIF", &gif),
        ];
        for (_, data) in pages {
            assert_eq!(
                ImagePage::decode(data).unwrap().fit(2, 1, 0).unwrap(),
                [0, 255]
            );
        }
        let fixture = Fixture::new("cbz");
        comic(&fixture.0, pages);
        let document = Comic::open(&fixture.0).unwrap();
        assert_eq!(document.page_count(), pages.len());
        for index in 0..pages.len() {
            assert_eq!(
                document.decode_page(index).unwrap().fit(2, 1, 0).unwrap(),
                [0, 255]
            );
        }
        assert!(ImagePage::decode(b"P5\n4097 1\n255\n\x00").is_err());
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
