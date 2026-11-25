//! Presentational HTML hints emitted against the historical normalized MOBI
//! text. Ranges let a long paragraph retain its active font on both sides of a cut.
use super::markup::{Addition, attribute};
use super::{MAX_HTML_DEPTH, MobiError};
use html5ever::tokenizer::{Tag, TagKind};
use std::{collections::BTreeMap, ops::Range};

const MAX_HINTS: usize = 32_768;
const SIZES: [&str; 7] = ["0.75", "0.88671875", "1", "1.19921875", "1.5", "2", "3"];

#[derive(Clone, Default, Debug)]
pub(crate) struct Presentation {
    pub(crate) aligned: BTreeMap<usize, &'static str>,
    sizes: Vec<(Range<usize>, usize)>,
    stack: Vec<Option<usize>>,
}

fn font_size(value: &str) -> Option<usize> {
    let value = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let mut chars = value.bytes().peekable();
    let sign = match chars.peek() {
        Some(b'+') | Some(b'-') => chars.next(),
        _ => None,
    };
    let mut digits = chars.take_while(u8::is_ascii_digit);
    let first = digits.next()?;
    // Seven is the largest meaningful absolute or relative magnitude.
    let number = digits.fold(first - b'0', |n, d| (n * 10 + d - b'0').min(9));
    let size = match sign {
        Some(b'+') => 3 + number as i32,
        Some(b'-') => 3 - number as i32,
        _ => number as i32,
    };
    Some(size.clamp(1, 7) as usize - 1)
}

impl Presentation {
    pub(crate) fn is_empty(&self) -> bool {
        self.aligned.is_empty() && self.sizes.is_empty()
    }

    pub(super) fn font(&mut self, tag: &Tag) -> Result<(), MobiError> {
        if tag.kind == TagKind::EndTag {
            self.stack.pop();
        } else if !tag.self_closing {
            if self.stack.len() >= MAX_HTML_DEPTH {
                return Err(MobiError::TextTooLarge);
            }
            let size = attribute(tag, "size")
                .and_then(font_size)
                .or_else(|| self.stack.last().copied().flatten());
            self.stack.push(size);
        }
        Ok(())
    }

    pub(super) fn alignment(&mut self, tag: &Tag, name: &str, at: usize) -> Result<(), MobiError> {
        let value = if &*tag.name == "center" {
            "center"
        } else if matches!(
            name,
            "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "td" | "th"
        ) {
            attribute(tag, "align").unwrap_or("")
        } else {
            return Ok(());
        };
        if let Some(value) = ["left", "right", "center", "justify"]
            .into_iter()
            .find(|v| value.eq_ignore_ascii_case(v))
        {
            if self.aligned.len() >= MAX_HINTS {
                return Err(MobiError::TextTooLarge);
            }
            self.aligned.insert(at, value);
        }
        Ok(())
    }

    pub(super) fn text(&mut self, range: Range<usize>) -> Result<(), MobiError> {
        if let Some(size) = self
            .stack
            .last()
            .copied()
            .flatten()
            .filter(|_| !range.is_empty())
        {
            if let Some((last, previous)) = self.sizes.last_mut()
                && *previous == size
                && last.end == range.start
            {
                last.end = range.end;
            } else {
                if self.sizes.len() >= MAX_HINTS {
                    return Err(MobiError::TextTooLarge);
                }
                self.sizes.push((range, size));
            }
        }
        Ok(())
    }

    /// Merge hints into one existing section, keeping all closing tags ahead of
    /// the next opening tag and reopening a text span after a mid-paragraph split.
    pub(crate) fn edits(&self, range: Range<usize>, existing: &[Addition]) -> Vec<Addition> {
        let mut edits = existing.to_vec();
        edits.extend(
            self.aligned
                .range(range.clone())
                .map(|(&at, alignment)| Addition {
                    at,
                    xml: format!(" style=\"text-align:{alignment}\""),
                }),
        );
        let start = self
            .sizes
            .partition_point(|(span, _)| span.end <= range.start);
        for (span, size) in self.sizes[start..]
            .iter()
            .take_while(|(span, _)| span.start < range.end)
        {
            edits.push(Addition {
                at: span.start.max(range.start),
                xml: format!("<span style=\"font-size:{}rem\">", SIZES[*size]),
            });
            edits.push(Addition {
                at: span.end.min(range.end),
                xml: "</span>".into(),
            });
        }
        edits.sort_by_key(|item| item.at);
        edits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an unencrypted original MOBI and private publisher-marked passages"]
    fn local_legacy_mobi_publisher_geometry() {
        use crate::{Engine, Probe, Settings, epub::Book};
        use std::{env, fs, path::PathBuf};
        let path = PathBuf::from(env::var_os("READER_TEST_MOBI").unwrap());
        let font = PathBuf::from(env::var_os("READER_TEST_MOBI_FONT").unwrap());
        let cases = fs::read_to_string(env::var_os("READER_TEST_MOBI_PASSAGES").unwrap()).unwrap();
        let samples: Vec<_> = cases
            .lines()
            .map(|line| line.split_once('\t').unwrap())
            .collect();
        let chapters = Book::open(&path).unwrap().chapters.len();
        let mut measurements = vec![Vec::new(); samples.len()];
        for chapter in 0..chapters {
            for styled in [false, true] {
                let settings = Settings {
                    publisher_styles: styled,
                    ..Settings::default()
                };
                let probe = Probe::open(
                    &path,
                    Engine::Parley,
                    std::slice::from_ref(&font),
                    settings,
                    chapter,
                )
                .unwrap();
                for (i, (_, sample)) in samples.iter().enumerate() {
                    for start in probe.matches(sample) {
                        let mut end = start.clone();
                        end.offset += sample.len();
                        let selected = probe
                            .select_range(&start, &end)
                            .expect("source-backed selection");
                        assert_eq!(selected.text.len(), sample.len());
                        let page = probe.page_for(&start).unwrap();
                        let boxes = probe.selection_rects(page, &selected);
                        let Some(first) = boxes.first() else { continue };
                        let left = boxes.iter().map(|r| r.x).min().unwrap();
                        let right = boxes.iter().map(|r| r.x + r.width).max().unwrap();
                        let total = boxes.iter().map(|r| r.width).sum::<i32>();
                        let top = boxes.iter().map(|r| r.y).min().unwrap();
                        let bottom = boxes.iter().map(|r| r.y + r.height).max().unwrap();
                        measurements[i].push(serde_json::json!({"styled":styled,"chapter":chapter,"page":page,"lines":boxes.iter().map(|r|r.y).collect::<std::collections::BTreeSet<_>>().len(),"left":left,"right":right,"width":total,"first_height":first.height,"height":bottom-top}));
                    }
                }
            }
        }
        for (i, values) in measurements.iter().enumerate() {
            println!(
                "geometry {} {} {}",
                i + 1,
                samples[i].0,
                serde_json::to_string(values).unwrap()
            );
        }
        assert!(measurements.iter().all(|values| !values.is_empty()));
    }

    #[test]
    fn publisher_hints_survive_invalid_and_nested_markup_and_mid_paragraph_splits() {
        use crate::{
            kf8::Kf8Chapter,
            kindle_sections,
            mobi::{Encoding, normalize_html},
        };
        let long = "wind ".repeat(44_000);
        let raw = format!(
            "<p align='CENTER'><font size='+2'>Outer <b>bold</b> {long}<font size='4'>inner</font> after</font></p><center>Centered block</center><p align='wrong'>Left</p>"
        );
        let (bare, mut features, _) = normalize_html(&raw, Some(Encoding::Utf8)).unwrap();
        let hints = std::mem::take(&mut features.presentation);
        assert!(!bare.contains("style="));
        let chapter = Kf8Chapter {
            path: "book.xhtml".into(),
            label: "Notes".into(),
            continued: false,
            xhtml: bare,
        };
        let (parts, old) = kindle_sections::legacy(chapter, Vec::new(), hints, &[], true).unwrap();
        assert!(parts.len() > 1);
        let mut wind_parts = 0;
        let mut seen = Vec::new();
        for part in &parts {
            let newer = roxmltree::Document::parse(&part.xhtml).unwrap();
            for word in newer.descendants().filter(|n| n.is_text()) {
                let parent = word.parent().unwrap();
                match word.text().unwrap() {
                    text if text.contains("wind") => {
                        assert_eq!(parent.attribute("style"), Some("font-size:1.5rem"));
                        assert_eq!(
                            parent
                                .ancestors()
                                .find(|n| n.has_tag_name("p"))
                                .unwrap()
                                .attribute("style"),
                            Some("text-align:center")
                        );
                        wind_parts += 1;
                    }
                    "inner" => {
                        assert_eq!(parent.attribute("style"), Some("font-size:1.19921875rem"));
                        seen.push("inner");
                    }
                    " after" => {
                        assert_eq!(parent.attribute("style"), Some("font-size:1.5rem"));
                        seen.push("after");
                    }
                    "Centered block" => {
                        assert_eq!(parent.attribute("style"), Some("text-align:center"));
                        seen.push("center");
                    }
                    "Left" => {
                        assert_eq!(parent.attribute("style"), None);
                        seen.push("left");
                    }
                    _ => {}
                }
            }
            for prior in &old[&part.path] {
                assert!(!prior.contains("style="));
                let mut current = crate::source::Source::parse(&part.xhtml).unwrap();
                current.accept_previous_mobi(prior).unwrap();
                let previous = crate::source::Source::parse(prior).unwrap();
                for (a, b) in previous.blocks.iter().zip(&current.blocks) {
                    assert_eq!(a.text, b.text);
                    assert_eq!(current.current_key(a.key, previous.revision), Some(b.key));
                }
            }
        }
        assert!(wind_parts >= 2);
        assert_eq!(seen, ["inner", "after", "center", "left"]);
    }

    #[test]
    fn legacy_font_sizes_use_seven_root_relative_steps_and_clamp_signed_values() {
        for (value, expected) in [
            ("1", 1),
            ("0", 1),
            (" 5extra", 5),
            ("4", 4),
            ("+1", 4),
            ("-1", 2),
            ("-0", 3),
            ("-4", 1),
            ("+222222222222", 7),
            ("99999999999999", 7),
        ] {
            assert_eq!(font_size(value).map(|n| n + 1), Some(expected), "{value}");
        }
        for value in ["", "+", "--1", "x2", " + 2"] {
            assert_eq!(font_size(value), None);
        }
    }
}
