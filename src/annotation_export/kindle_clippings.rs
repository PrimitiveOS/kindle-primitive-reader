//! Offline Kindle highlights, notes and bookmarks, independent of Reader anchors.

use crate::{mark_time, reading_stats::history::date::Date};
use std::fmt::Write;
use std::io;

pub(crate) const FILE: &str = "My Clippings.txt";
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RECORDS: usize = 8_192;
const SEPARATOR: &str = "==========";
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Highlight,
    Note,
    Bookmark,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Highlight => "highlight",
            Self::Note => "note",
            Self::Bookmark => "bookmark",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Highlight => "Highlight",
            Self::Note => "Note",
            Self::Bookmark => "Bookmark",
        }
    }

    fn parse(label: &str) -> Option<Self> {
        if label.contains("highlight") || label.contains("标注") {
            Some(Self::Highlight)
        } else if label.contains("note") || label.contains("笔记") {
            Some(Self::Note)
        } else if label.contains("bookmark") || label.contains("书签") {
            Some(Self::Bookmark)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Location<'a> {
    Page(u64),
    Position(&'a str),
    Chapter(&'a str),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Clip<'a> {
    pub(crate) title: &'a str,
    pub(crate) author: Option<&'a str>,
    pub(crate) kind: Kind,
    pub(crate) location: Option<Location<'a>>,
    pub(crate) time: Option<i64>,
    pub(crate) text: &'a str,
}

pub(crate) fn parse(source: &[u8]) -> io::Result<Vec<Clip<'_>>> {
    let source = std::str::from_utf8(source)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Kindle clippings must be UTF-8"))?
        .trim_start_matches('\u{feff}');
    let mut clips = Vec::new();
    let mut start = 0;
    let mut end = 0;
    for line in source.split_inclusive('\n') {
        let line_start = end;
        end += line.len();
        if line.trim() == SEPARATOR {
            if let Some(clip) = parse_record(&source[start..line_start]) {
                if clips.len() == MAX_RECORDS {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Too many Kindle clippings to export at once",
                    ));
                }
                clips.push(clip);
            }
            start = end;
        }
    }
    Ok(clips)
}

fn parse_record(record: &str) -> Option<Clip<'_>> {
    let mut lines = record.trim_start_matches(['\r', '\n']).splitn(4, '\n');
    let (title, author) = title_author(lines.next()?.trim());
    let header = lines.next()?;
    if !lines.next()?.trim().is_empty() || header.len() > 4096 {
        return None;
    }
    let text = lines.next().unwrap_or_default().trim();
    let mut fields = header.split('|');
    let description = fields.next()?;
    let kind = Kind::parse(&description.to_ascii_lowercase())?;
    if title.is_empty() || (text.is_empty() && kind != Kind::Bookmark) {
        return None;
    }
    let local_chapter = description
        .split_once(" on ")
        .map(|(_, location)| location.trim())
        .filter(|location| location.starts_with("Chapter "))
        .map(Location::Chapter);
    let location = local_chapter.or_else(|| parse_location(header));
    Some(Clip {
        title,
        author,
        kind,
        location,
        time: fields.last().and_then(parse_time),
        text,
    })
}

fn parse_location(header: &str) -> Option<Location<'_>> {
    header
        .split('|')
        .find_map(|field| {
            let page = number_after(field, &["page ", "页码", "第"])?;
            if field.contains('第') && !field.contains('页') {
                return None;
            }
            page.parse::<u64>()
                .ok()
                .filter(|page| *page > 0)
                .map(Location::Page)
        })
        .or_else(|| {
            header.split('|').find_map(|field| {
                number_after(field, &["location ", "loc. ", "位置"]).map(Location::Position)
            })
        })
}

fn title_author(title: &str) -> (&str, Option<&str>) {
    let title = match title.rsplit_once('.') {
        Some((stem, extension))
            if ["pdf", "djvu", "epub", "fb2", "mobi", "txt", "html", "doc"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known)) =>
        {
            stem
        }
        _ => title,
    };
    if let Some((title, author)) = title
        .strip_suffix(')')
        .and_then(|title| title.rsplit_once('('))
    {
        let title = title.trim_end_matches(|c: char| c.is_whitespace() || c == '-');
        let author = author.trim();
        return (title, (!author.is_empty()).then_some(author));
    }
    if let Some((title, author)) = title.rsplit_once(" - ")
        && !author.trim().is_empty()
    {
        return (title.trim(), Some(author.trim()));
    }
    (title, None)
}

fn number_after<'a>(field: &'a str, keys: &[&str]) -> Option<&'a str> {
    let lower = field.to_ascii_lowercase();
    let offset = keys
        .iter()
        .find_map(|key| lower.find(key).map(|index| index + key.len()))?;
    let rest = field[offset..].trim_start();
    let length = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_digit() || *byte == b'-')
        .count();
    let number = &rest[..length];
    let (start, end) = number.split_once('-').unwrap_or((number, number));
    (start.parse::<u64>().ok()? > 0 && end.parse::<u64>().ok()? > 0).then_some(number)
}

fn parse_time(value: &str) -> Option<i64> {
    let words: Vec<_> = value.split_whitespace().collect();
    let clock_index = words
        .iter()
        .position(|word| word.matches(':').count() == 2)?;
    let mut clock = words[clock_index]
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split(':');
    let hour = clock.next()?.parse::<u8>().ok()?;
    let minute = clock.next()?.parse::<u8>().ok()?;
    let second = clock.next()?.parse::<u8>().ok()?;
    let clock_prefix = &words[..=clock_index];
    let midday = if clock_prefix.iter().any(|word| word.contains("下午")) {
        Some("PM")
    } else if clock_prefix.iter().any(|word| word.contains("上午")) {
        Some("AM")
    } else {
        words.get(clock_index + 1).copied()
    };
    let hour = match midday {
        Some("AM") if (1..=12).contains(&hour) => hour % 12,
        Some("PM") if (1..=12).contains(&hour) => hour % 12 + 12,
        Some("AM" | "PM") => return None,
        _ => hour,
    };
    let date = words.get(clock_index.checked_sub(1)?)?;
    let iso_date = if date.len() == 10 && date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-' {
        (*date).to_owned()
    } else if let Some(date) = words[..clock_index].iter().find(|word| word.contains('年')) {
        let date = date.trim_start_matches(|c: char| !c.is_ascii_digit());
        let (year, rest) = date.split_once('年')?;
        let (month, rest) = rest.split_once('月')?;
        let (day, _) = rest.split_once('日')?;
        format!(
            "{year}-{:02}-{:02}",
            month.parse::<u8>().ok()?,
            day.parse::<u8>().ok()?
        )
    } else {
        let date_words = words.get(clock_index.checked_sub(3)?..clock_index)?;
        let month = MONTHS
            .iter()
            .position(|month| *month == date_words[0] || &month[..3] == date_words[0])?
            + 1;
        let day = date_words[1].trim_end_matches(',').parse::<u8>().ok()?;
        format!("{}-{month:02}-{day:02}", date_words[2])
    };
    mark_time::parse(format!("{iso_date} {hour:02}:{minute:02}:{second:02}").as_bytes())
}

pub(crate) fn write(
    output: &mut String,
    title: &str,
    author: Option<&str>,
    kind: Kind,
    location: Option<&str>,
    time: Option<i64>,
    text: &str,
) {
    let title = title.replace(['\r', '\n'], " ");
    let author = author.unwrap_or("Unknown").replace(['\r', '\n'], " ");
    let time = time.and_then(clipping_date);
    let _ = writeln!(output, "{title} ({author})");
    let _ = write!(output, "- Your {}", kind.title());
    if let Some(location) = location {
        let _ = write!(output, " on {}", location.replace(['\r', '\n'], " "));
    }
    let _ = writeln!(
        output,
        " | Added on {}\n",
        time.as_deref().unwrap_or("Unknown date")
    );
    let _ = writeln!(output, "{text}\n{SEPARATOR}");
}

pub(crate) fn write_highlight(
    output: &mut String,
    title: &str,
    author: Option<&str>,
    location: Option<&str>,
    time: Option<i64>,
    text: &str,
    note: Option<&str>,
) {
    write(output, title, author, Kind::Highlight, location, time, text);
    if let Some(note) = note {
        write(output, title, author, Kind::Note, location, time, note);
    }
}

fn clipping_date(time: i64) -> Option<String> {
    let clock = mark_time::local_parts(time)?;
    let month = u8::try_from(clock.month).ok()?;
    let day = u8::try_from(clock.day).ok()?;
    let date = Date::new(clock.year, month, day)?;
    let weekday = WEEKDAYS[date.weekday() as usize];
    let month = MONTHS[usize::from(month - 1)];
    let hour = (clock.hour + 11) % 12 + 1;
    let midday = if clock.hour < 12 { "AM" } else { "PM" };
    Some(format!(
        "{weekday}, {month} {day:02}, {} {hour:02}:{:02}:{:02} {midday}",
        clock.year, clock.minute, clock.second
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_multiline_clippings_and_local_noon_and_midnight() {
        let data = concat!(
            "\u{feff}Harbor (Field Notes) (Linh Nguyen).epub\r\n",
            "- Your Highlight on page 4 | Location 68-73 | Added on Thursday, February 29, 2024 12:03:04 PM\r\n",
            "\r\nThe harbor stays quiet.\r\nSecond line.\r\n==========\r\n",
            "港口笔记 - 李明\r\n- 笔记 位置 98-102 | 添加于 2024年2月29日 上午12:03:04\r\n",
            "\r\n换一行。\r\n==========\r\n",
            "Harbor (Field Notes) (Linh Nguyen)\r\n- Your Bookmark on Location 101 | Added on 2024-02-29 21:02:03\r\n",
            "\r\n\r\n==========\r\n",
            "Partial\r\n- Your Highlight on page 2 | Added on Unknown date\r\n\r\nIncomplete"
        );
        let found = parse(data.as_bytes()).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(
            found[0],
            Clip {
                title: "Harbor (Field Notes)",
                author: Some("Linh Nguyen"),
                kind: Kind::Highlight,
                location: Some(Location::Page(4)),
                time: mark_time::parse(b"2024-02-29 12:03:04"),
                text: "The harbor stays quiet.\r\nSecond line.",
            }
        );
        assert_eq!(found[1].author, Some("李明"));
        assert_eq!(found[1].kind, Kind::Note);
        assert_eq!(found[1].location, Some(Location::Position("98-102")));
        assert_eq!(found[1].time, mark_time::parse(b"2024-02-29 00:03:04"));
        assert_eq!(found[2].kind, Kind::Bookmark);
        assert_eq!(found[2].location, Some(Location::Position("101")));
        assert!(found[2].text.is_empty());
    }

    #[test]
    fn round_trips_clipping_text_and_preserves_missing_dates() {
        let mut output = String::new();
        let local = mark_time::parse(b"2024-02-29 00:03:04").unwrap();
        write(
            &mut output,
            "Field notes",
            Some("Linh Nguyen"),
            Kind::Highlight,
            Some("page 8"),
            Some(local),
            "One line.\nAnother line.",
        );
        write(
            &mut output,
            "Field notes",
            Some("Linh Nguyen"),
            Kind::Note,
            Some("Location 19-22"),
            None,
            "Undated detail",
        );
        let parsed = parse(output.as_bytes()).unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(output.contains("Thursday, February 29, 2024 12:03:04 AM"));
        assert_eq!(parsed[0].location, Some(Location::Page(8)));
        assert_eq!(parsed[0].time, Some(local));
        assert_eq!(parsed[0].text, "One line.\nAnother line.");
        assert_eq!(parsed[1].location, Some(Location::Position("19-22")));
        assert_eq!(parsed[1].time, None);
    }

    #[test]
    fn rejects_non_utf8_and_over_limit_inputs() {
        assert!(parse(b"\xff").is_err());
        let record = "Harbor\n- Your Note | Added on Unknown date\n\nA note.\n==========\n";
        assert!(parse(record.repeat(MAX_RECORDS + 1).as_bytes()).is_err());
    }
}
