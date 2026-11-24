//! Bounded Wikimedia thumbnails for the existing contextual lookup.
use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{self, Cursor, Read};
use std::process::Command;
use std::time::Duration;

use base64::prelude::*;
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer,
};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use super::{Job, MAX_RESPONSE, agent, language, live_fetch};

const UPLOAD: &str = "upload.wikimedia.org";
const MAX_URL: usize = 1024;
const MAX_CAPTION: usize = 240;
const MAX_EXTRA: usize = 12;
pub(crate) const MAX_PICTURES: usize = 4;
const MAX_SIDE: u32 = 480;
const MAX_DECODE_SIDE: u32 = 2048;
const DECODE_BYTES: u64 = 24 * 1024 * 1024;
const MAX_WORKER_REPLY: u64 =
    (MAX_SIDE as u64 * MAX_SIDE as u64 * 4 / 3 + 4096) * MAX_PICTURES as u64 + 4096;
const MEDIA_TIMEOUT: Duration = Duration::from_secs(5);
const MEDIA_JOB_LIMIT: Duration = Duration::from_secs(32);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Link {
    url: String,
    name: String,
    caption: String,
}

impl Link {
    fn new(source: &str, name: Option<&str>, caption: &str) -> Option<Self> {
        let url = media_url(source)?;
        let name = name
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| file_name(&url))?;
        Some(Self {
            url: url.into(),
            name: clean(&name),
            caption: clean(caption),
        })
    }
}

pub(super) fn thumbnail(page: &Value) -> Option<Link> {
    Link::new(
        page.pointer("/thumbnail/source")?.as_str()?,
        page.get("pageimage").and_then(Value::as_str),
        "",
    )
}

fn media_url(text: &str) -> Option<Url> {
    if text.len() > MAX_URL || text.chars().any(char::is_control) {
        return None;
    }
    let url = if text.starts_with("//") {
        Url::parse(&format!("https:{text}")).ok()?
    } else {
        Url::parse(text).ok()?
    };
    (url.scheme() == "https"
        && url.host_str() == Some(UPLOAD)
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path().starts_with("/wikipedia/"))
    .then_some(url)
}

fn file_name(url: &Url) -> Option<String> {
    let mut segments = url.path_segments()?.rev();
    let last = segments.next()?;
    let file = if url.path().contains("/thumb/") {
        segments.next()?
    } else {
        last
    };
    let decoded = percent_decode_str(file).decode_utf8().ok()?;
    (!decoded.is_empty()).then(|| decoded.into_owned())
}

fn clean(text: &str) -> String {
    let mut result = String::new();
    let mut space = false;
    for ch in text.chars().take(MAX_CAPTION * 4) {
        if result.len() + ch.len_utf8() + usize::from(space && !result.is_empty()) > MAX_CAPTION {
            break;
        }
        if ch.is_whitespace() || ch.is_control() {
            space = true;
            continue;
        }
        if space && !result.is_empty() {
            result.push(' ');
        }
        result.push(ch);
        space = false;
    }
    result
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Picture {
    pub width: u32,
    pub height: u32,
    #[serde(with = "pixel_data")]
    pub pixels: Vec<u8>,
    pub caption: String,
    pub name: String,
}

impl Picture {
    pub(crate) fn valid(&self) -> bool {
        (1..=MAX_SIDE).contains(&self.width)
            && (1..=MAX_SIDE).contains(&self.height)
            && self.pixels.len() == self.width as usize * self.height as usize
            && self.name.len() <= MAX_CAPTION
            && self.caption.len() <= MAX_CAPTION
    }
}

mod pixel_data {
    use super::*;

    pub(super) fn serialize<S: serde::Serializer>(pixels: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&BASE64_STANDARD.encode(pixels))
    }

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let data = String::deserialize(d)?;
        if data.len() > (MAX_SIDE as usize * MAX_SIDE as usize).div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("Image data is too large"));
        }
        BASE64_STANDARD
            .decode(data)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Request {
    pub title: String,
    pub language: String,
    pub lead: Option<Link>,
    pub more: bool,
}

impl Request {
    fn valid(&self) -> bool {
        language(&self.language) && !self.title.trim().is_empty() && self.title.len() <= 1024
    }
}

fn parsed_article(request: &Request) -> Url {
    let mut url = super::wiki_api(&request.language);
    url.query_pairs_mut().extend_pairs([
        ("action", "parse"),
        ("format", "json"),
        ("formatversion", "2"),
        ("prop", "text"),
        ("page", &request.title),
        ("redirects", "1"),
        ("disableeditsection", "1"),
        ("disablelimitreport", "1"),
    ]);
    url
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Figure,
    Gallery,
}

struct Card {
    group: Group,
    image: Option<String>,
    caption: String,
    depth: usize,
    caption_depth: Option<usize>,
}

#[derive(Default)]
struct Results {
    card: Option<Card>,
    figures: Vec<Link>,
    gallery: Vec<Link>,
    raw: bool,
}

#[derive(Default)]
struct ImageTags(RefCell<Results>);

fn attr<'a>(tag: &'a Tag, name: &str) -> Option<&'a str> {
    tag.attrs
        .iter()
        .find(|attr| &*attr.name.local == name)
        .map(|attr| attr.value.as_ref())
}

fn class(tag: &Tag, name: &str) -> bool {
    attr(tag, "class")
        .is_some_and(|classes| classes.split_ascii_whitespace().any(|entry| entry == name))
}

fn void(tag: &str) -> bool {
    matches!(
        tag,
        "img"
            | "br"
            | "hr"
            | "wbr"
            | "input"
            | "embed"
            | "area"
            | "source"
            | "link"
            | "meta"
            | "col"
    )
}

impl TokenSink for ImageTags {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut result = self.0.borrow_mut();
        if let Token::CharacterTokens(text) = token {
            if result.raw {
                return TokenSinkResult::Continue;
            }
            if let Some(card) = result.card.as_mut()
                && card.caption_depth.is_some()
                && card.caption.len() < MAX_CAPTION * 4
            {
                let cap =
                    text.floor_char_boundary(text.len().min(MAX_CAPTION * 4 - card.caption.len()));
                card.caption.push_str(&text[..cap]);
            }
            return TokenSinkResult::Continue;
        }
        let Token::TagToken(tag) = token else {
            return TokenSinkResult::Continue;
        };
        let name = tag.name.as_ref();
        if tag.kind == TagKind::StartTag {
            if matches!(name, "script" | "style") {
                result.raw = true;
                return TokenSinkResult::RawData(if name == "script" {
                    html5ever::tokenizer::states::RawKind::ScriptData
                } else {
                    html5ever::tokenizer::states::RawKind::Rawtext
                });
            }
            if let Some(card) = result.card.as_mut() {
                if !void(name) && !tag.self_closing {
                    card.depth += 1;
                }
                if name == "figcaption" || (name == "div" && class(&tag, "gallerytext")) {
                    card.caption_depth = Some(card.depth);
                }
                if name == "img"
                    && card.image.is_none()
                    && ["width", "height"].iter().all(|name| {
                        attr(&tag, name)
                            .and_then(|value| value.parse::<u32>().ok())
                            .is_some_and(|n| n > 0)
                    })
                {
                    card.image = attr(&tag, "src")
                        .filter(|src| media_url(src).is_some())
                        .map(str::to_owned);
                }
            } else if let Some(group) = match name {
                "figure"
                    if attr(&tag, "typeof").is_some_and(|value| {
                        value
                            .split_ascii_whitespace()
                            .any(|part| matches!(part, "mw:File/Thumb" | "mw:File/Frame"))
                    }) && result.figures.len() < MAX_EXTRA =>
                {
                    Some(Group::Figure)
                }
                "li" if class(&tag, "gallerybox") && result.gallery.len() < MAX_EXTRA => {
                    Some(Group::Gallery)
                }
                _ => None,
            } {
                result.card = Some(Card {
                    group,
                    image: None,
                    caption: String::new(),
                    depth: 1,
                    caption_depth: None,
                });
            }
        } else if matches!(name, "script" | "style") {
            result.raw = false;
        } else if let Some(card) = result.card.as_mut() {
            if card.caption_depth == Some(card.depth) {
                card.caption_depth = None;
            }
            card.depth = card.depth.saturating_sub(1);
            if card.depth == 0 {
                let card = result.card.take().expect("active image card");
                if let Some(img) = card
                    .image
                    .and_then(|src| Link::new(&src, None, &card.caption))
                {
                    match card.group {
                        Group::Figure => result.figures.push(img),
                        Group::Gallery => result.gallery.push(img),
                    }
                }
            }
        }
        TokenSinkResult::Continue
    }
}

fn more_images(json: &Value) -> Vec<Link> {
    let Some(html) = json
        .pointer("/parse/text")
        .and_then(|text| {
            text.as_str()
                .or_else(|| text.get("*").and_then(Value::as_str))
        })
        .filter(|html| html.len() as u64 <= MAX_RESPONSE)
    else {
        return Vec::new();
    };
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from_slice(html));
    let tokenizer = Tokenizer::new(ImageTags::default(), Default::default());
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    let result = tokenizer.sink.0.into_inner();
    result.figures.into_iter().chain(result.gallery).collect()
}

fn ordered(lead: Option<Link>, extra: Vec<Link>) -> Vec<Link> {
    fn key(link: &Link) -> String {
        link.name.replace(' ', "_")
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    if let Some(mut lead) = lead {
        if let Some(replacement) = extra.iter().find(|extra| key(extra) == key(&lead)) {
            // The article figure supplies its caption. Retain the chosen lead
            // thumbnail URL, which was already sized by PageImages.
            lead.caption.clone_from(&replacement.caption);
        }
        seen.insert(key(&lead));
        out.push(lead);
    }
    for extra in extra {
        if seen.insert(key(&extra)) {
            out.push(extra);
        }
    }
    out
}

fn decode(bytes: &[u8], link: Link) -> Option<Picture> {
    if bytes.len() as u64 > MAX_RESPONSE {
        return None;
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)
    ) {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(DECODE_BYTES);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if !(1..=MAX_DECODE_SIDE).contains(&width)
        || !(1..=MAX_DECODE_SIDE).contains(&height)
        || decoder.total_bytes() > DECODE_BYTES
    {
        return None;
    }
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    let image = image.thumbnail(MAX_SIDE, MAX_SIDE).into_luma_alpha8();
    let pixels = image
        .pixels()
        .map(|pixel| {
            let [gray, alpha] = pixel.0.map(u16::from);
            ((gray * alpha + 255 * (255 - alpha) + 127) / 255) as u8
        })
        .collect();
    Some(Picture {
        width: image.width(),
        height: image.height(),
        pixels,
        caption: link.caption,
        name: link.name,
    })
}

fn resolve(
    request: Request,
    mut fetch_metadata: impl FnMut(Url) -> Result<Value, String>,
    mut fetch_image: impl FnMut(Url) -> Result<Vec<u8>, String>,
) -> Vec<Picture> {
    if !request.valid() {
        return Vec::new();
    }
    let extra = if request.more {
        fetch_metadata(parsed_article(&request))
            .map_or_else(|_| Vec::new(), |json| more_images(&json))
    } else {
        Vec::new()
    };
    ordered(request.lead, extra)
        .into_iter()
        .filter_map(|link| media_url(&link.url).map(|url| (url, link)))
        .take(MAX_PICTURES)
        .filter_map(|(url, link)| decode(&fetch_image(url).ok()?, link))
        .collect()
}

pub(super) fn worker() -> crate::Result<()> {
    let request: Request = serde_json::from_reader(io::stdin().lock().take(8192))?;
    let agent = agent(MEDIA_TIMEOUT);
    let pictures = resolve(
        request,
        |url| live_fetch(url, &agent),
        |url| {
            let mut response = agent
                .get(url.as_str())
                .call()
                .map_err(|_| "Image unavailable")?;
            if response.status().as_u16() != 200 {
                return Err("Image unavailable".into());
            }
            response
                .body_mut()
                .with_config()
                .limit(MAX_RESPONSE)
                .read_to_vec()
                .map_err(|_| "Image response too large or incomplete".into())
        },
    );
    serde_json::to_writer(io::stdout().lock(), &Ok::<_, String>(pictures))?;
    Ok(())
}

pub(crate) type ImageJob = Job<Vec<Picture>>;

impl ImageJob {
    #[cfg(test)]
    pub(crate) fn fixture(command: Command, timeout: Duration) -> crate::Result<Self> {
        Self::spawn_payload(command, b"{}\n".to_vec(), MAX_WORKER_REPLY, timeout)
    }

    pub(crate) fn start_images(request: &Request) -> crate::Result<Self> {
        if !request.valid() {
            return Err("Image query is unavailable".into());
        }
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--wiki-images-worker");
        Self::spawn_payload(
            command,
            serde_json::to_vec(request)?,
            MAX_WORKER_REPLY,
            MEDIA_JOB_LIMIT,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ARTICLE: &str = include_str!("../../tests/fixtures/wikipedia-article-media.html");
    const LEAD: &str =
        "https://upload.wikimedia.org/wikipedia/commons/thumb/4/44/Harbor.png/250px-Harbor.png";

    fn request(more: bool) -> Request {
        Request {
            title: "Harbor notebook".into(),
            language: "en".into(),
            lead: Link::new(LEAD, Some("Harbor.png"), ""),
            more,
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let im = image::RgbaImage::from_pixel(width, height, image::Rgba([30, 30, 30, 100]));
        let mut output = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(im)
            .write_to(&mut output, ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }

    #[test]
    fn wikipedia_media_rejects_unapproved_hosts_urls_and_plain_article_images() {
        for url in [
            "http://upload.wikimedia.org/wikipedia/a.png",
            "https://upload.wikimedia.org.example.invalid/wikipedia/a.png",
            "https://upload.wikimedia.org@other.invalid/wikipedia/a.png",
            "https://login@upload.wikimedia.org/wikipedia/a.png",
            "https://upload.wikimedia.org:5443/wikipedia/a.png",
            "https://upload.wikimedia.org/wikipedia/a.png?x=3",
            "https://upload.wikimedia.org/wikipedia/a.png#preview",
            "https://upload.wikimedia.org/archive/a.png",
            "file:///mnt/us/documents/a.png",
            "/relative/a.png",
        ] {
            assert!(media_url(url).is_none(), "{url}");
        }
        assert!(media_url("//upload.wikimedia.org/wikipedia/a.png").is_some());
        let additional = more_images(&json!({"parse":{"text":ARTICLE}}));
        assert_eq!(
            additional
                .iter()
                .map(|link| link.name.as_str())
                .collect::<Vec<_>>(),
            ["Jetty.png", "Harbor.png", "Signal.png"]
        );
        assert_eq!(additional[0].caption, "A jetty & channel.");
    }

    #[test]
    fn wikipedia_media_orders_lead_figures_and_gallery_and_preserves_source_captions() {
        let mut requested = Vec::new();
        let encoded = png(140, 90);
        let photos = resolve(
            request(true),
            |url| {
                let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
                assert_eq!(url.host_str(), Some("en.wikipedia.org"));
                assert_eq!(params.get("action").map(|v| v.as_ref()), Some("parse"));
                assert_eq!(
                    params.get("page").map(|v| v.as_ref()),
                    Some("Harbor notebook")
                );
                Ok(json!({"parse":{"text":{"*":ARTICLE}}}))
            },
            |url| {
                requested.push(file_name(&url).unwrap());
                Ok(encoded.clone())
            },
        );
        assert_eq!(requested, ["Harbor.png", "Jetty.png", "Signal.png"]);
        assert_eq!(photos[0].caption, "Lead harbor illustration.");
        assert_eq!(photos[2].caption, "Signal flag in the gallery.");
        assert!(photos.iter().all(Picture::valid));
        assert!(
            photos[0]
                .pixels
                .iter()
                .all(|&pixel| pixel > 150 && pixel < 255)
        );
        let serial = serde_json::to_string(&photos).unwrap();
        let replay: Vec<Picture> = serde_json::from_str(&serial).unwrap();
        assert_eq!(replay[0].pixels, photos[0].pixels);
    }

    #[test]
    fn wikipedia_media_drops_large_and_missing_assets_and_recovers_the_lead_without_parse() {
        let good = png(120, 60);
        let too_wide = png(MAX_DECODE_SIDE + 1, 1);
        let mut called = 0;
        let photos = resolve(
            request(true),
            |_| Ok(json!({"parse":{"text":ARTICLE}})),
            |_| {
                called += 1;
                match called {
                    1 => Err("Image timed out".into()),
                    2 => Ok(vec![0; MAX_RESPONSE as usize + 1]),
                    3 => Ok(too_wide.clone()),
                    _ => unreachable!(),
                }
            },
        );
        assert_eq!(called, 3);
        assert!(photos.is_empty());

        let fallback = resolve(
            request(true),
            |_| Err("Article parsed response is too large".into()),
            |_| Ok(good.clone()),
        );
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].name, "Harbor.png");
        let intro = resolve(
            request(false),
            |_| panic!("Introduction fetched full markup"),
            |_| Ok(good.clone()),
        );
        assert_eq!(intro.len(), 1);
    }

    #[test]
    fn wikipedia_media_never_fetches_a_forged_ipc_source() {
        let mut forged = request(false);
        forged.lead.as_mut().unwrap().url = "https://other.invalid/wikipedia/a.png".into();
        assert!(
            resolve(
                forged,
                |_| unreachable!(),
                |_| panic!("Unapproved image URL was used")
            )
            .is_empty()
        );
    }
}
