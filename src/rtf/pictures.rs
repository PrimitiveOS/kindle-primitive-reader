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

#[derive(Default)]
struct Payload {
    format: Option<image::ImageFormat>,
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
        let reader = crate::image_reader(&self.bytes)?;
        if reader.format() != Some(format) {
            return Err("The RTF image type does not match its data".into());
        }
        let (width, height) = reader.into_dimensions()?;
        if !crate::comic::image_dimensions_valid(width, height) {
            return Err("The RTF image exceeds the decoded image limit".into());
        }
        Ok(Picture {
            extension: if format == image::ImageFormat::Png {
                "png"
            } else {
                "jpg"
            },
            bytes: self.bytes,
        })
    }
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
                        b"pngblip" => Some(image::ImageFormat::Png),
                        b"jpegblip" => Some(image::ImageFormat::Jpeg),
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
