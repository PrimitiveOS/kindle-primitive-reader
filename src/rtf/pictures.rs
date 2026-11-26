//! Bounded image-only recovery after the ordinary RTF parser has accepted all
//! syntax and reported that the document has no supported reading text.
use super::{Lexer, Token, hidden_destination, trim_space};
use crate::Result;

const MAX_PICTURES: usize = 16;
const MAX_PICTURE_BYTES: usize = 1024 * 1024;
const MAX_PICTURE_TOTAL: usize = 4 * 1024 * 1024;

#[derive(Debug)]
struct NoPictures;

impl std::fmt::Display for NoPictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The RTF contains no supported image-only content")
    }
}

impl std::error::Error for NoPictures {}

pub(crate) struct Picture {
    pub(crate) extension: &'static str,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone, Copy, Default)]
struct Group {
    hidden: bool,
    hidden_text: bool,
    optional: bool,
    first: bool,
    shape: bool,
    shape_child: bool,
    shape_children: u8,
    picture_started: bool,
}

#[derive(Clone, Copy)]
enum PictureFormat {
    Png,
    Jpeg,
    Dib,
}

#[derive(Default)]
struct Payload {
    format: Option<PictureFormat>,
    bytes: Vec<u8>,
    high: Option<u8>,
    binary: bool,
    hex: bool,
}

impl Payload {
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_PICTURE_BYTES.saturating_sub(self.bytes.len()) {
            return Err("The RTF image exceeds its size limit".into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn text(&mut self, bytes: &[u8]) -> Result<()> {
        for &byte in bytes {
            if byte.is_ascii_whitespace() {
                continue;
            }
            if self.binary || self.format.is_none() {
                return Err("The RTF image has an ambiguous payload".into());
            }
            let digit = super::hex(byte).ok_or("The RTF image has invalid hex data")?;
            self.hex = true;
            if let Some(high) = self.high.take() {
                self.append(&[high * 16 + digit])?;
            } else {
                self.high = Some(digit);
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<Picture> {
        if self.high.is_some() || self.bytes.is_empty() {
            return Err("The RTF image has incomplete data".into());
        }
        let format = self.format.ok_or("The RTF image type is unsupported")?;
        let (bytes, expected, extension) = match format {
            PictureFormat::Png => (self.bytes, image::ImageFormat::Png, "png"),
            PictureFormat::Jpeg => (self.bytes, image::ImageFormat::Jpeg, "jpg"),
            PictureFormat::Dib => (dib_to_bmp(self.bytes)?, image::ImageFormat::Bmp, "bmp"),
        };
        let reader = crate::image_reader(&bytes)?;
        if reader.format() != Some(expected) {
            return Err("The RTF image type does not match its data".into());
        }
        let (width, height) = reader.into_dimensions()?;
        if !crate::comic::image_dimensions_valid(width, height) {
            return Err("The RTF image exceeds the decoded image limit".into());
        }
        Ok(Picture { extension, bytes })
    }
}

// RTF DIBs omit the 14-byte BMP file header. Restrict recovery to exact,
// uncompressed BITMAPINFOHEADER layouts and validate every palette index.
fn dib_to_bmp(dib: Vec<u8>) -> Result<Vec<u8>> {
    const INFO_HEADER: usize = 40;
    const FILE_HEADER: usize = 14;
    if dib.len() < INFO_HEADER || dib.len() > MAX_PICTURE_BYTES - FILE_HEADER {
        return Err("The RTF DIB image exceeds its size limit or has an incomplete header".into());
    }
    let short = |at: usize| u16::from_le_bytes([dib[at], dib[at + 1]]);
    let long = |at: usize| u32::from_le_bytes([dib[at], dib[at + 1], dib[at + 2], dib[at + 3]]);
    let width = i32::from_le_bytes([dib[4], dib[5], dib[6], dib[7]]);
    let height = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let bits = short(14);
    let rows = height.unsigned_abs();
    if long(0) != INFO_HEADER as u32
        || width <= 0
        || height == 0
        || short(12) != 1
        || !matches!(bits, 1 | 4 | 8 | 24)
        || long(16) != 0
        || !crate::comic::image_dimensions_valid(width as u32, rows)
    {
        return Err("The RTF DIB image uses an unsupported bitmap layout".into());
    }
    let colors = long(32);
    let important = long(36);
    let palette_count = if bits == 24 {
        if colors != 0 || important != 0 {
            return Err("The RTF RGB DIB has an unsupported palette".into());
        }
        0usize
    } else {
        let maximum = 1u32 << bits;
        let count = if colors == 0 { maximum } else { colors };
        if count > maximum || important > count {
            return Err("The RTF DIB palette is invalid".into());
        }
        usize::try_from(count)?
    };
    let palette_bytes = palette_count
        .checked_mul(4)
        .ok_or("The RTF DIB palette overflowed")?;
    let pixel_start = INFO_HEADER
        .checked_add(palette_bytes)
        .ok_or("The RTF DIB palette overflowed")?;
    let palette = dib
        .get(INFO_HEADER..pixel_start)
        .ok_or("The RTF DIB palette is incomplete")?;
    if palette.chunks_exact(4).any(|color| color[3] != 0) {
        return Err("The RTF DIB palette has unsupported reserved data".into());
    }
    let row = (width as usize)
        .checked_mul(bits as usize)
        .and_then(|value| value.checked_add(31))
        .map(|value| value / 32 * 4)
        .ok_or("The RTF DIB row size overflowed")?;
    let pixels = row
        .checked_mul(rows as usize)
        .ok_or("The RTF DIB size overflowed")?;
    if pixel_start.checked_add(pixels) != Some(dib.len())
        || (long(20) != 0 && usize::try_from(long(20)).ok() != Some(pixels))
    {
        return Err("The RTF DIB pixel data is incomplete or ambiguous".into());
    }
    if palette_count != 0 {
        let mask = ((1u16 << bits) - 1) as u8;
        for scanline in dib[pixel_start..].chunks_exact(row) {
            for column in 0..width as usize {
                let at = column * bits as usize;
                let shift = 8 - bits as usize - at % 8;
                if usize::from((scanline[at / 8] >> shift) & mask) >= palette_count {
                    return Err("The RTF DIB has an invalid palette index".into());
                }
            }
        }
    }
    let total = FILE_HEADER
        .checked_add(dib.len())
        .ok_or("The RTF DIB size overflowed")?;
    let mut bmp = Vec::with_capacity(total);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&u32::try_from(total)?.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&u32::try_from(FILE_HEADER + pixel_start)?.to_le_bytes());
    bmp.extend_from_slice(&dib);
    Ok(bmp)
}

pub(crate) fn image_only(input: &[u8]) -> Result<Vec<Picture>> {
    match scan(input, false) {
        Err(error) if error.is::<NoPictures>() => scan(input, true),
        result => result,
    }
}

fn scan(input: &[u8], allow_shape: bool) -> Result<Vec<Picture>> {
    let input = input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input);
    let mut lexer = Lexer {
        remaining: trim_space(input),
    };
    let mut groups: Vec<Group> = Vec::new();
    let mut active: Option<(usize, Payload)> = None;
    let mut pictures = Vec::new();
    let mut total = 0usize;
    while let Some(token) = lexer.next()? {
        match token {
            Token::Open => {
                if active.is_some() {
                    return Err("Nested RTF picture groups are unsupported".into());
                }
                let parent = groups.last().copied().unwrap_or_default();
                if parent.shape_child {
                    return Err("Nested RTF shape picture groups are unsupported".into());
                }
                if parent.shape {
                    let wrapper = groups.last_mut().unwrap();
                    wrapper.shape_children += 1;
                    if wrapper.shape_children != 1 {
                        return Err("The RTF shape picture has ambiguous children".into());
                    }
                }
                groups.push(Group {
                    hidden: parent.hidden || parent.optional,
                    hidden_text: parent.hidden_text,
                    optional: false,
                    first: true,
                    shape: false,
                    shape_child: parent.shape,
                    shape_children: 0,
                    picture_started: false,
                });
            }
            Token::Close => {
                if active
                    .as_ref()
                    .is_some_and(|(depth, _)| *depth == groups.len())
                {
                    let (_, payload) = active.take().unwrap();
                    let picture = payload.finish()?;
                    total = total
                        .checked_add(picture.bytes.len())
                        .ok_or("RTF image size overflow")?;
                    if pictures.len() >= MAX_PICTURES || total > MAX_PICTURE_TOTAL {
                        return Err("The RTF document exceeds its image budget".into());
                    }
                    pictures.push(picture);
                }
                let group = groups.pop().ok_or("Unbalanced RTF group")?;
                if (group.shape && group.shape_children != 1)
                    || (group.shape_child && !group.picture_started)
                {
                    return Err("The RTF shape picture has no unique direct picture".into());
                }
            }
            Token::Word(word, number) => {
                let group = groups.last_mut().ok_or("RTF content outside a group")?;
                if let Some((_, payload)) = active.as_mut() {
                    let format = match word {
                        b"pngblip" => Some(PictureFormat::Png),
                        b"jpegblip" => Some(PictureFormat::Jpeg),
                        b"dibitmap" if number == Some(0) => Some(PictureFormat::Dib),
                        _ => None,
                    };
                    if let Some(format) = format {
                        if payload.format.replace(format).is_some()
                            || !payload.bytes.is_empty()
                            || payload.high.is_some()
                        {
                            return Err("The RTF image type is ambiguous".into());
                        }
                    }
                } else if allow_shape
                    && group.first
                    && word == b"shppict"
                    && !group.hidden
                    && !group.hidden_text
                    && !group.shape_child
                {
                    group.shape = true;
                    group.optional = false;
                } else if group.first
                    && word == b"pict"
                    && !group.hidden
                    && !group.hidden_text
                    && !group.optional
                {
                    group.picture_started = true;
                    active = Some((groups.len(), Payload::default()));
                } else if group.shape || group.shape_child {
                    return Err("The RTF shape picture contains unsupported content".into());
                } else if group.optional || hidden_destination(word) || word == b"upr" {
                    group.hidden = true;
                    group.optional = false;
                } else if word == b"v" {
                    group.hidden_text = number != Some(0);
                } else if word == b"plain" {
                    group.hidden_text = false;
                }
                if let Some(group) = groups.last_mut() {
                    group.first = false;
                }
            }
            Token::Text(bytes) => {
                if let Some((_, payload)) = active.as_mut() {
                    payload.text(bytes)?;
                }
                if let Some(group) = groups.last_mut() {
                    if bytes.iter().any(|b| !b.is_ascii_whitespace()) {
                        if group.shape || (group.shape_child && active.is_none()) {
                            return Err("The RTF shape picture contains unsupported text".into());
                        }
                        group.first = false;
                    }
                }
            }
            Token::Binary(bytes) => {
                if let Some((_, payload)) = active.as_mut() {
                    if payload.hex || payload.binary || payload.format.is_none() {
                        return Err("The RTF image has an ambiguous binary payload".into());
                    }
                    payload.binary = true;
                    payload.append(bytes)?;
                } else if groups
                    .last()
                    .is_some_and(|group| group.shape || group.shape_child)
                {
                    return Err("The RTF shape picture has an unsupported binary segment".into());
                }
            }
            Token::Symbol(symbol) => {
                if active.is_some() {
                    return Err("The RTF image has an unsupported escaped payload".into());
                }
                if groups
                    .last()
                    .is_some_and(|group| group.shape || group.shape_child)
                {
                    return Err("The RTF shape picture has an unsupported control".into());
                }
                if symbol == b'*'
                    && let Some(group) = groups.last_mut()
                {
                    group.optional = true;
                }
            }
            Token::Byte(_) => {
                if active.is_some()
                    || groups
                        .last()
                        .is_some_and(|group| group.shape || group.shape_child)
                {
                    return Err("The RTF image has an unsupported escaped payload".into());
                }
            }
        }
    }
    if active.is_some() || !groups.is_empty() {
        return Err("The RTF picture groups are incomplete".into());
    }
    if pictures.is_empty() {
        return Err(Box::new(NoPictures));
    }
    Ok(pictures)
}
