//! Bounded text conversion for RTF 1, based on Microsoft's RTF 1.9.1 specification.
//! Document character sets, Unicode escapes, paragraph breaks and inline emphasis
//! are translated to XHTML. This adapter neither opens resources nor runs fields.
//! https://officeprotocoldoc.z19.web.core.windows.net/files/Archive_References/%5BMSFT-RTF%5D.pdf

use encoding_rs as enc;

use crate::Result;

mod pictures;

#[derive(Debug)]
pub(crate) struct NoReadableText;

impl std::fmt::Display for NoReadableText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The RTF document contains no supported reading text")
    }
}

impl std::error::Error for NoReadableText {}

pub(crate) use pictures::image_only;

pub(crate) const MAX_INPUT: usize = 4 * 1024 * 1024;
const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_FALLBACK: usize = 32;
const MAX_MARKUP_NODES: usize = 32_000;

pub(crate) fn decode(input: &[u8]) -> Result<String> {
    if input.len() > MAX_INPUT {
        return Err("The RTF document exceeds Reader's 4 MiB input limit".into());
    }
    let input = input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input);
    let mut tokens = Lexer {
        remaining: trim_space(input),
    };
    if !matches!(tokens.next()?, Some(Token::Open))
        || !matches!(tokens.next()?, Some(Token::Word(b"rtf", Some(1))))
    {
        return Err("This file does not begin with an RTF 1 header".into());
    }
    let mut parser = Parser::new();
    while let Some(token) = tokens.next()? {
        parser.accept(token)?;
        if parser.groups.is_empty() {
            if !trim_space(tokens.remaining).is_empty() {
                return Err("The RTF document contains data after its final group".into());
            }
            return parser.output.finish();
        }
    }
    Err("The RTF document ends before its last group is closed".into())
}

fn trim_space(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}

enum Token<'a> {
    Open,
    Close,
    Word(&'a [u8], Option<i32>),
    Symbol(u8),
    Byte(u8),
    Text(&'a [u8]),
    Binary(&'a [u8]),
}

struct Lexer<'a> {
    remaining: &'a [u8],
}

impl<'a> Lexer<'a> {
    fn next(&mut self) -> Result<Option<Token<'a>>> {
        // Literal source newlines are layout whitespace, unlike \line and \par.
        let end = self
            .remaining
            .iter()
            .position(|byte| !matches!(byte, b'\r' | b'\n'))
            .unwrap_or(self.remaining.len());
        self.remaining = &self.remaining[end..];
        let input = self.remaining;
        let Some((&first, tail)) = input.split_first() else {
            return Ok(None);
        };
        self.remaining = tail;
        let token = match first {
            b'{' => Token::Open,
            b'}' => Token::Close,
            b'\\' => self.control()?,
            _ => {
                let count = tail
                    .iter()
                    .take_while(|byte| !matches!(byte, b'{' | b'}' | b'\\' | b'\r' | b'\n'))
                    .count();
                self.remaining = &tail[count..];
                Token::Text(&input[..count + 1])
            }
        };
        Ok(Some(token))
    }

    fn control(&mut self) -> Result<Token<'a>> {
        let (&first, tail) = self
            .remaining
            .split_first()
            .ok_or("The RTF control escape is truncated")?;
        if !first.is_ascii_alphabetic() {
            self.remaining = tail;
            if first == b'\'' {
                let (digits, tail) = self
                    .remaining
                    .split_at_checked(2)
                    .ok_or("The RTF hex escape is truncated")?;
                let high = hex(digits[0]).ok_or("The RTF hex escape is invalid")?;
                let low = hex(digits[1]).ok_or("The RTF hex escape is invalid")?;
                self.remaining = tail;
                return Ok(Token::Byte(high * 16 + low));
            }
            if first == b'\r' {
                self.remaining = self.remaining.strip_prefix(b"\n").unwrap_or(self.remaining);
            }
            return Ok(Token::Symbol(first));
        }
        let length = self
            .remaining
            .iter()
            .take_while(|byte| byte.is_ascii_alphabetic())
            .count();
        if length > 32 {
            return Err("The RTF control word is too long".into());
        }
        let (word, tail) = self.remaining.split_at(length);
        let sign =
            usize::from(tail.first() == Some(&b'-') && tail.get(1).is_some_and(u8::is_ascii_digit));
        let digits = tail[sign..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let number = if digits == 0 {
            None
        } else {
            if digits > 10 {
                return Err("The RTF control number is too large".into());
            }
            Some(std::str::from_utf8(&tail[..sign + digits])?.parse::<i32>()?)
        };
        let tail = &tail[sign + digits..];
        self.remaining = tail.strip_prefix(b" ").unwrap_or(tail);
        if word == b"bin" {
            let length = number
                .and_then(|value| usize::try_from(value).ok())
                .ok_or("The RTF binary segment has an invalid length")?;
            let (data, tail) = self
                .remaining
                .split_at_checked(length)
                .ok_or("The RTF binary segment is truncated")?;
            self.remaining = tail;
            Ok(Token::Binary(data))
        } else {
            Ok(Token::Word(word, number))
        }
    }
}

fn hex(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|digit| digit as u8)
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Emphasis {
    bold: bool,
    italic: bool,
}

mod oem_tables;

#[derive(Clone, Copy)]
enum TextEncoding {
    Standard(&'static enc::Encoding),
    Cp437,
    Cp850,
    Oem(&'static [char; 128]),
}

#[derive(Clone, Copy)]
struct Group {
    style: Emphasis,
    encoding: Option<TextEncoding>,
    fallback: usize,
    hidden: bool,
    hidden_text: bool,
    optional: bool,
    unicode_alternate: bool,
    alternates: Option<u8>,
}

struct Parser {
    groups: Vec<Group>,
    output: Output,
    encoded: Vec<u8>,
    skip: usize,
    high_surrogate: Option<u16>,
}

impl Parser {
    fn new() -> Self {
        Self {
            groups: vec![Group {
                style: Emphasis::default(),
                encoding: Some(TextEncoding::Standard(enc::WINDOWS_1252)),
                fallback: 1,
                hidden: false,
                hidden_text: false,
                optional: false,
                unicode_alternate: false,
                alternates: None,
            }],
            output: Output::new(),
            encoded: Vec::new(),
            skip: 0,
            high_surrogate: None,
        }
    }

    fn group(&self) -> Group {
        *self.groups.last().unwrap()
    }

    fn current(&mut self) -> &mut Group {
        self.groups.last_mut().unwrap()
    }

    fn accept(&mut self, token: Token<'_>) -> Result<()> {
        match token {
            Token::Open => {
                self.boundary()?;
                self.skip = 0;
                if self.groups.len() == MAX_DEPTH {
                    return Err("The RTF group nesting is too deep".into());
                }
                let mut child = self.group();
                child.hidden |= child.optional;
                if let Some(branches) = &mut self.current().alternates {
                    *branches = branches.saturating_add(1);
                    child.hidden |= *branches != 2;
                    child.unicode_alternate = *branches == 2 && !child.hidden;
                } else {
                    child.unicode_alternate = false;
                }
                child.optional = false;
                child.alternates = None;
                self.groups.push(child);
            }
            Token::Close => {
                self.boundary()?;
                self.skip = 0;
                self.groups.pop();
            }
            Token::Text(bytes) => {
                let skipped = self.skip.min(bytes.len());
                self.skip -= skipped;
                self.text_bytes(&bytes[skipped..])?;
            }
            _ if self.skip > 0 => self.skip -= 1,
            Token::Word(word, number) => self.word(word, number)?,
            Token::Byte(byte) => self.text_bytes(&[byte])?,
            Token::Symbol(symbol) => self.symbol(symbol)?,
            Token::Binary(_) => (),
        }
        Ok(())
    }

    fn text_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let group = self.group();
        if !bytes.is_empty()
            && !group.hidden
            && !group.optional
            && !group.hidden_text
            && group.alternates.is_none()
        {
            self.finish_surrogate()?;
            if self.encoded.len() + bytes.len() > MAX_OUTPUT {
                return Err("The RTF text exceeds Reader's output limit".into());
            }
            self.encoded.extend_from_slice(bytes);
        }
        Ok(())
    }

    fn flush_bytes(&mut self) -> Result<()> {
        if self.encoded.is_empty() {
            return Ok(());
        }
        let state = self.group();
        let text = if self.encoded.is_ascii() {
            std::borrow::Cow::Borrowed(std::str::from_utf8(&self.encoded)?)
        } else {
            match state
                .encoding
                .ok_or("The RTF document uses an unsupported text code page")?
            {
                TextEncoding::Standard(charset) => charset
                    .decode_without_bom_handling_and_without_replacement(&self.encoded)
                    .ok_or("An RTF text run contains invalid bytes for its code page")?,
                mode @ (TextEncoding::Cp437 | TextEncoding::Cp850 | TextEncoding::Oem(_)) => {
                    let table = match mode {
                        TextEncoding::Cp437 => &oem_tables::CP437,
                        TextEncoding::Cp850 => &oem_tables::CP850,
                        TextEncoding::Oem(table) => table,
                        TextEncoding::Standard(_) => unreachable!(),
                    };
                    let mut decoded = String::with_capacity(self.encoded.len());
                    for &byte in &self.encoded {
                        decoded.push(if byte < 128 {
                            char::from(byte)
                        } else {
                            table[usize::from(byte - 128)]
                        });
                        if decoded.len() > MAX_OUTPUT {
                            return Err("The RTF text exceeds Reader's output limit".into());
                        }
                    }
                    std::borrow::Cow::Owned(decoded)
                }
            }
        };
        self.output.text(&text, state.style)?;
        self.encoded.clear();
        Ok(())
    }

    fn finish_surrogate(&self) -> Result<()> {
        if self.high_surrogate.is_some() {
            return Err("An RTF Unicode surrogate pair is incomplete".into());
        }
        Ok(())
    }

    fn boundary(&mut self) -> Result<()> {
        self.finish_surrogate()?;
        self.flush_bytes()
    }

    fn unicode(&mut self, number: Option<i32>) -> Result<()> {
        self.flush_bytes()?;
        let unit = number
            .filter(|&number| (-32768..=65535).contains(&number))
            .ok_or("The RTF Unicode escape is outside the UTF-16 range")? as u16;
        let scalar = match unit {
            0xd800..=0xdbff => {
                self.finish_surrogate()?;
                self.high_surrogate = Some(unit);
                return Ok(());
            }
            0xdc00..=0xdfff => {
                let lead = self
                    .high_surrogate
                    .take()
                    .ok_or("The RTF Unicode escape has no leading surrogate")?;
                0x10000 + ((u32::from(lead) - 0xd800) << 10) + (u32::from(unit) - 0xdc00)
            }
            _ => {
                self.finish_surrogate()?;
                u32::from(unit)
            }
        };
        let character = char::from_u32(scalar).ok_or("The RTF Unicode escape is invalid")?;
        let style = self.group().style;
        self.output.text(character.encode_utf8(&mut [0; 4]), style)
    }

    fn symbol(&mut self, symbol: u8) -> Result<()> {
        let state = self.group();
        if state.hidden || state.optional {
            return Ok(());
        }
        if symbol == b'*' {
            self.boundary()?;
            self.current().optional = true;
            return Ok(());
        }
        if state.hidden_text {
            return Ok(());
        }
        match symbol {
            b'{' | b'}' | b'\\' => self.text_bytes(&[symbol]),
            b'~' => self.visible("\u{00a0}"),
            b'_' => self.visible("\u{2011}"),
            b'-' => self.visible("\u{00ad}"),
            b'\r' | b'\n' => self.paragraph(),
            _ => Ok(()),
        }
    }

    fn word(&mut self, word: &[u8], number: Option<i32>) -> Result<()> {
        let state = self.group();
        if state.hidden {
            return Ok(());
        }
        if state.optional {
            self.current().optional = false;
            self.current().hidden = !(state.unicode_alternate && word == b"ud");
            return Ok(());
        }
        if hidden_destination(word) {
            self.boundary()?;
            self.current().hidden = true;
            return Ok(());
        }
        match word {
            b"upr" => self.current().alternates = Some(0),
            b"uc" => {
                self.current().fallback = number
                    .and_then(|number| usize::try_from(number).ok())
                    .filter(|count| *count <= MAX_FALLBACK)
                    .ok_or("The RTF Unicode fallback count is unsupported")?;
            }
            b"u" => {
                if !state.hidden_text {
                    self.unicode(number)?;
                }
                self.skip = state.fallback;
            }
            b"ansi" | b"mac" | b"pc" | b"pca" | b"ansicpg" => {
                self.boundary()?;
                self.current().encoding = match word {
                    b"ansi" => Some(TextEncoding::Standard(enc::WINDOWS_1252)),
                    b"mac" => Some(TextEncoding::Standard(enc::MACINTOSH)),
                    b"pc" => Some(TextEncoding::Cp437),
                    b"pca" => Some(TextEncoding::Cp850),
                    b"ansicpg" => code_page(number.ok_or("The RTF code page is missing")?),
                    _ => None,
                };
            }
            b"b" | b"i" | b"v" | b"plain" => {
                self.boundary()?;
                let enabled = number != Some(0);
                let state = self.current();
                match word {
                    b"b" => state.style.bold = enabled,
                    b"i" => state.style.italic = enabled,
                    b"v" => state.hidden_text = enabled,
                    _ => {
                        state.style = Emphasis::default();
                        state.hidden_text = false;
                    }
                }
            }
            _ if state.hidden_text => (),
            b"par" | b"sect" | b"page" | b"row" | b"nestrow" => self.paragraph()?,
            b"line" => {
                self.boundary()?;
                self.output.line()?;
            }
            b"tab" | b"cell" | b"nestcell" => self.visible("\t")?,
            b"emdash" => self.visible("\u{2014}")?,
            b"endash" => self.visible("\u{2013}")?,
            b"bullet" => self.visible("\u{2022}")?,
            b"lquote" => self.visible("\u{2018}")?,
            b"rquote" => self.visible("\u{2019}")?,
            b"ldblquote" => self.visible("\u{201c}")?,
            b"rdblquote" => self.visible("\u{201d}")?,
            b"emspace" => self.visible("\u{2003}")?,
            b"enspace" => self.visible("\u{2002}")?,
            _ => (),
        }
        Ok(())
    }

    fn visible(&mut self, text: &str) -> Result<()> {
        self.boundary()?;
        self.output.text(text, self.group().style)
    }

    fn paragraph(&mut self) -> Result<()> {
        self.boundary()?;
        self.output.paragraph()
    }
}

fn hidden_destination(word: &[u8]) -> bool {
    matches!(
        word,
        b"fonttbl"
            | b"filetbl"
            | b"colortbl"
            | b"stylesheet"
            | b"info"
            | b"listtable"
            | b"listoverridetable"
            | b"revtbl"
            | b"rsidtbl"
            | b"generator"
            | b"header"
            | b"headerf"
            | b"headerl"
            | b"headerr"
            | b"footer"
            | b"footerf"
            | b"footerl"
            | b"footerr"
            | b"footnote"
            | b"annotation"
            | b"fldinst"
            | b"htmltag"
            | b"object"
            | b"objdata"
            | b"pict"
            | b"shppict"
            | b"nonshppict"
            | b"shp"
            | b"shpinst"
            | b"datastore"
            | b"datafield"
            | b"themedata"
    )
}

fn code_page(number: i32) -> Option<TextEncoding> {
    match number {
        437 => return Some(TextEncoding::Cp437),
        850 => return Some(TextEncoding::Cp850),
        852 => return Some(TextEncoding::Oem(&oem_tables::CP852)),
        860 => return Some(TextEncoding::Oem(&oem_tables::CP860)),
        863 => return Some(TextEncoding::Oem(&oem_tables::CP863)),
        865 => return Some(TextEncoding::Oem(&oem_tables::CP865)),
        _ => {}
    }
    Some(TextEncoding::Standard(match number {
        866 => enc::IBM866,
        874 => enc::WINDOWS_874,
        932 => enc::SHIFT_JIS,
        936 => enc::GBK,
        949 => enc::EUC_KR,
        950 => enc::BIG5,
        1250 => enc::WINDOWS_1250,
        1251 => enc::WINDOWS_1251,
        1252 => enc::WINDOWS_1252,
        1253 => enc::WINDOWS_1253,
        1254 => enc::WINDOWS_1254,
        1255 => enc::WINDOWS_1255,
        1256 => enc::WINDOWS_1256,
        1257 => enc::WINDOWS_1257,
        1258 => enc::WINDOWS_1258,
        10000 => enc::MACINTOSH,
        10007 => enc::X_MAC_CYRILLIC,
        20866 => enc::KOI8_R,
        21866 => enc::KOI8_U,
        51932 => enc::EUC_JP,
        54936 => enc::GB18030,
        65001 => enc::UTF_8,
        _ => return None,
    }))
}

struct Output {
    xhtml: String,
    style: Emphasis,
    open_paragraph: bool,
    has_text: bool,
    markup_nodes: usize,
}

impl Output {
    fn new() -> Self {
        Self {
            xhtml: String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body>"),
            style: Emphasis::default(),
            open_paragraph: false,
            has_text: false,
            markup_nodes: 0,
        }
    }

    fn markup(&mut self, text: &str) -> Result<()> {
        self.markup_nodes += 1;
        if self.markup_nodes > MAX_MARKUP_NODES {
            return Err("The RTF document has too many layout elements".into());
        }
        self.xhtml.push_str(text);
        self.check_size()
    }

    fn begin_paragraph(&mut self) -> Result<()> {
        if !self.open_paragraph {
            self.markup("<p>")?;
            self.open_paragraph = true;
        }
        Ok(())
    }

    fn close_style(&mut self) {
        if self.style.italic {
            self.xhtml.push_str("</em>");
        }
        if self.style.bold {
            self.xhtml.push_str("</strong>");
        }
        self.style = Emphasis::default();
    }

    fn text(&mut self, text: &str, style: Emphasis) -> Result<()> {
        self.begin_paragraph()?;
        if self.style != style {
            self.close_style();
            if style.bold {
                self.markup("<strong>")?;
            }
            if style.italic {
                self.markup("<em>")?;
            }
            self.style = style;
        }
        self.has_text |= !text.trim().is_empty();
        for character in text.chars() {
            match character {
                '&' => self.xhtml.push_str("&amp;"),
                '<' => self.xhtml.push_str("&lt;"),
                '>' => self.xhtml.push_str("&gt;"),
                '\t' => self.xhtml.push_str("\u{00a0}\u{00a0}\u{00a0}\u{00a0}"),
                '\n' | '\r' => self.markup("<br />")?,
                '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}' => {
                    self.xhtml.push(character)
                }
                _ => return Err("An RTF text run contains an unsupported control character".into()),
            }
            self.check_size()?;
        }
        Ok(())
    }

    fn line(&mut self) -> Result<()> {
        self.begin_paragraph()?;
        self.markup("<br />")
    }

    fn paragraph(&mut self) -> Result<()> {
        if self.open_paragraph {
            self.close_style();
            self.xhtml.push_str("</p>");
            self.open_paragraph = false;
            self.check_size()
        } else {
            self.markup("<p>&#160;</p>")
        }
    }

    fn check_size(&self) -> Result<()> {
        if self.xhtml.len() > MAX_OUTPUT {
            return Err("The converted RTF document exceeds Reader's output limit".into());
        }
        Ok(())
    }

    fn finish(mut self) -> Result<String> {
        if !self.has_text {
            return Err(Box::new(NoReadableText));
        }
        if self.open_paragraph {
            self.paragraph()?;
        }
        self.xhtml.push_str("</body></html>");
        self.check_size()?;
        Ok(self.xhtml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;

    #[test]
    fn preserves_paragraphs_tabs_lines_and_scoped_emphasis() {
        let xhtml = decode(br"{\rtf1\ansi Lead {\b bold {\i both} back} regular.\par One\tab Two\line Three <&> \{literal\}\\}").unwrap();
        let source = Source::parse(&xhtml).unwrap();
        assert_eq!(source.blocks.len(), 2);
        assert_eq!(source.blocks[0].text, "Lead bold both back regular.");
        assert_eq!(
            source.blocks[1].text,
            "One\u{a0}\u{a0}\u{a0}\u{a0}Two\nThree <&> {literal}\\"
        );
        let first = &source.blocks[0];
        for (word, bold, italic) in [
            ("bold", true, false),
            ("both", true, true),
            ("back", true, false),
            ("regular", false, false),
        ] {
            let start = first.text.find(word).unwrap();
            let (_, style) = first
                .emphasis
                .iter()
                .find(|(range, _)| range.contains(&start))
                .unwrap();
            assert_eq!((style.bold, style.italic), (bold, italic), "{word}");
        }
        assert!(xhtml.contains("&lt;&amp;&gt;"));
    }

    #[test]
    fn decodes_code_pages_unicode_surrogates_and_scoped_fallbacks() {
        let xhtml = decode(br"{\rtf1\ansi\ansicpg1251 \'cf\'f0\'e8\'e2\'e5\'f2\par{\ansicpg932 \'82\'a0\'82\'a2}\par \uc1\u272?i\u7879?n \u1593?\u1604? \u-10179?\u-8704?\par{\uc2\u233\'e9\~Z}\u244? {\upr{fallback}{\*\ud\uc0\u214}}}").unwrap();
        let source = Source::parse(&xhtml).unwrap();
        let paragraphs: Vec<_> = source
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect();
        assert_eq!(paragraphs, ["Привет", "あい", "Điện عل 😀", "éZô Ö"]);
    }

    #[test]
    fn consumes_binary_data_and_ignores_resource_and_field_instruction_groups() {
        let mut bytes =
            br"{\rtf1\ansi{\fonttbl{\f0 Hidden font;}}{\info{\title Invisible}}First {\pict\bin4 "
                .to_vec();
        bytes.extend_from_slice(b"}\\{\0");
        bytes.extend_from_slice(
            br#"}{\object{\objdata unreachable}}{\*\newdestination skipped}{\field{\*\fldinst INCLUDETEXT "file:///private"}{\fldrslt shown}}{\v invisible} after}"#,
        );
        let xhtml = decode(&bytes).unwrap();
        let source = Source::parse(&xhtml).unwrap();
        assert_eq!(source.blocks[0].text, "First shown after");
        assert!(!xhtml.contains("file:"));
        assert!(!xhtml.contains("unreachable"));
    }

    #[test]
    fn rejects_truncated_and_bounded_malformed_rtf() {
        for document in [
            b"not RTF".as_slice(),
            br"{\rtf2 New version}",
            br"{\rtf1 Never closes",
            br"{\rtf1 text}extra",
            br"{\rtf1 \bin-1 nonsense}",
            br"{\rtf1{\pict\bin100 truncated}}",
            br"{\rtf1 \'ff \'z}",
            br"{\rtf1 \uc99\u100?}",
            br"{\rtf1 \uc0\u-10179 unmatched}",
            br"{\rtf1 \ansi\ansicpg932 \'82}",
            br"{\rtf1 \pc \'82}",
        ] {
            assert!(decode(document).is_err(), "{document:?}");
        }
        let nested = format!(
            "{{\\rtf1 {}deep{}}}",
            "{".repeat(MAX_DEPTH),
            "}".repeat(MAX_DEPTH)
        );
        assert!(decode(nested.as_bytes()).is_err());
        assert!(decode(&vec![b' '; MAX_INPUT + 1]).is_err());
        let expanded = format!("{{\\rtf1 {}}}", "&".repeat(MAX_OUTPUT / 4));
        assert!(decode(expanded.as_bytes()).is_err());
    }
}
