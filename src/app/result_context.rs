//! Search-result text and the page/contents information shown when a result is held.
use super::*;
use primitive_ui::TextStyle;
use std::{borrow::Cow, collections::HashMap, ops::Range};

pub(super) struct ResultContext {
    pub(super) label: String,
    pub(super) chapters: Vec<String>,
    first: usize,
}

const CONTEXT_ROWS: usize = 9;

impl ResultContext {
    fn new(label: String, chapters: Vec<String>) -> Self {
        // Start at the selected section. A swipe reveals earlier parent headings.
        let first = chapters.len().saturating_sub(CONTEXT_ROWS);
        Self {
            label,
            chapters,
            first,
        }
    }

    pub(super) fn scroll(&mut self, dy: f32) {
        let last = self.chapters.len().saturating_sub(CONTEXT_ROWS);
        self.first = if dy < 0.0 {
            self.first.saturating_add(CONTEXT_ROWS).min(last)
        } else {
            self.first.saturating_sub(CONTEXT_ROWS)
        };
    }

    pub(super) fn draw(&self, canvas: &mut Canvas) {
        let shown = self.chapters.iter().skip(self.first).take(CONTEXT_ROWS);
        let height = 124
            + 70 * shown.len() as i32
            + if self.chapters.len() > CONTEXT_ROWS {
                54
            } else {
                0
            };
        let panel = Rect::new(
            108,
            (HEIGHT as i32 - height) / 2,
            WIDTH as i32 - 216,
            height,
        );
        canvas.panel(panel, 18, 3, 255);
        canvas.text(
            Rect::new(panel.x + 34, panel.y + 22, panel.w - 144, 70),
            &self.label,
            BODY.bold(),
        );
        let close = Rect::new(panel.right() - 86, panel.y + 21, 48, 48);
        canvas.icon(close, bar_icons::CLOSE, 30);
        for (row, title) in shown.enumerate() {
            let y = panel.y + 102 + 70 * row as i32;
            let indent = 34 + 27 * (row as i32).min(5);
            let current = self.first + row + 1 == self.chapters.len();
            let square = if current { 12 } else { 6 };
            canvas.fill(
                Rect::new(panel.x + indent, y + 20, square, square),
                if current { 20 } else { 126 },
            );
            let style = if current { BODY.bold() } else { BODY };
            canvas.text(
                Rect::new(panel.x + indent + 28, y, panel.w - indent - 60, 64),
                title,
                style,
            );
        }
        if self.chapters.len() > CONTEXT_ROWS {
            canvas.text(
                Rect::new(panel.x + 34, panel.bottom() - 56, panel.w - 68, 45),
                &format!(
                    "{}–{} of {}",
                    self.first + 1,
                    self.first + CONTEXT_ROWS,
                    self.chapters.len()
                ),
                CAPTION,
            );
        }
    }
}

fn chapter_path<'a, T, K: Ord>(
    entries: &'a [T],
    target: K,
    first_at_target: bool,
    key: impl Fn(&T) -> Option<K>,
    heading: impl Fn(&'a T) -> (usize, &'a str),
) -> Vec<String> {
    let mut chosen = None;
    for (index, entry) in entries.iter().enumerate() {
        let Some(position) = key(entry) else { continue };
        if position > target {
            continue;
        }
        if first_at_target && position == target {
            chosen = Some((index, position));
            break;
        }
        if chosen
            .as_ref()
            .is_none_or(|(_, before)| &position >= before)
        {
            chosen = Some((index, position));
        }
    }
    let Some((index, _)) = chosen else {
        return vec![];
    };
    let mut path = Vec::new();
    let mut below = usize::MAX;
    for entry in entries[..=index].iter().rev() {
        let (depth, title) = heading(entry);
        if depth < below {
            if !title.is_empty() && path.last().is_none_or(|last| last != title) {
                path.push(title.to_owned());
            }
            if depth == 0 {
                break;
            }
            below = depth;
        }
    }
    path.reverse();
    path
}

/// Keep the matched word on screen even when its preceding CJK or wide text
/// would fill a complete row. All cuts are at extended grapheme boundaries.
pub(super) fn row_excerpt(
    text: &str,
    matched: Range<usize>,
    expand: bool,
    prefix_graphemes: usize,
) -> (Cow<'_, str>, Range<usize>) {
    let mut range = matched;
    if range.is_empty() || text.get(range.clone()).is_none() {
        return (Cow::Borrowed(text), 0..0);
    }
    if expand {
        let first = range.start;
        let last = range.end - 1;
        for word in crate::selection::word_ranges(text) {
            if word.contains(&first) {
                range.start = word.start;
            }
            if word.contains(&last) {
                range.end = word.end;
                break;
            }
            if word.start >= range.end {
                break;
            }
        }
    }
    if prefix_graphemes == usize::MAX {
        return (Cow::Borrowed(text), range);
    }
    let prefix = &text[..range.start];
    let breaks: Vec<_> = icu_segmenter::GraphemeClusterSegmenter::new()
        .segment_str(prefix)
        .collect();
    if breaks.len().saturating_sub(1) <= prefix_graphemes {
        return (Cow::Borrowed(text), range);
    }
    let tail = &breaks[breaks.len() - (prefix_graphemes + 1)..];
    let mut start = tail[0];
    if let Some(next) = tail
        .windows(2)
        .take_while(|pair| pair[0] - start <= 12)
        .find(|pair| prefix[pair[0]..pair[1]].starts_with(char::is_whitespace))
    {
        start = next[1];
    }
    let displayed = format!("…{}", &text[start..]);
    let shift = '…'.len_utf8();
    (
        Cow::Owned(displayed),
        range.start - start + shift..range.end - start + shift,
    )
}

/// Use the search engine's source coordinates, including inline fragment IDs.
/// Opening a result may paginate another chapter; previewing it must not do so.
fn reflow_context(
    active: &ActiveBook,
    hit: &SearchHit,
    custom: Option<&[Bookmark]>,
) -> Result<ResultContext> {
    let reading = &active.reading;
    let probe = &reading.probe;
    let current = hit.chapter == reading.chapter;
    let other;
    let source = if current {
        &probe.source
    } else {
        let xml = with_storage(|| {
            let mut book = Book::open(&reading.config.book).map_err(io::Error::other)?;
            book.chapter(hit.chapter).map_err(io::Error::other)
        })?;
        other = crate::source::Source::parse(&xml)?;
        &other
    };
    let (block_index, block) = source
        .blocks
        .iter()
        .enumerate()
        .find(|(_, block)| block.key == hit.anchor.block)
        .ok_or("The passage has moved")?;
    if source.revision != hit.anchor.revision || !block.text.is_char_boundary(hit.anchor.offset) {
        return Err("The passage has moved".into());
    }

    let at = (hit.chapter, hit.anchor.block, hit.anchor.offset);
    let order: HashMap<_, _> = probe
        .chapter_entries()
        .iter()
        .enumerate()
        .map(|(i, chapter)| (chapter.href.as_str(), i))
        .collect();
    let key = |entry: &crate::epub::TocEntry| {
        let dest = entry.destination.as_ref()?;
        let chapter = *order.get(dest.path.as_str())?;
        if chapter == hit.chapter
            && let Some(fragment) = &dest.fragment
        {
            let &(block, byte) = source.fragments.get(fragment)?;
            return Some((chapter, block, byte));
        }
        Some((chapter, 0, 0))
    };
    let reference_page = if let Some(map) = &active.numbering {
        map.chapters.get(hit.chapter).and_then(|chapter| {
            if chapter.revision != hit.anchor.revision {
                return None;
            }
            let end = chapter.points.partition_point(|point| {
                point.is_none_or(|p| (p.block, p.byte) <= (hit.anchor.block, hit.anchor.offset))
            });
            end.checked_sub(1).map(|i| chapter.first + i)
        })
    } else {
        probe
            .printed_pages
            .iter()
            .enumerate()
            .filter_map(|(row, entry)| {
                let position = key(entry)?;
                (position <= at).then_some((position, row))
            })
            .max()
            .map(|(_, row)| row)
            .or_else(|| (!probe.printed_pages.is_empty()).then_some(0))
    };
    let label = if let Some(reference) = reference_page.and_then(|row| active.page_label(row)) {
        format!("Page {reference}")
    } else if current {
        let page = probe.page_for(&hit.anchor).ok_or("The passage has moved")?;
        format!("Chapter page {} of {}", page + 1, probe.page_count())
    } else {
        let count = source
            .blocks
            .iter()
            .map(|b| b.text.chars().count())
            .sum::<usize>();
        let before = source.blocks[..block_index]
            .iter()
            .map(|b| b.text.chars().count())
            .sum::<usize>()
            + block.text[..hit.anchor.offset].chars().count();
        let percent = ((before as u64) * 100)
            .checked_div(count as u64)
            .unwrap_or(0);
        format!(
            "Chapter {} of {} · {percent}%",
            hit.chapter + 1,
            probe.chapter_count()
        )
    };
    let mut titles = if let Some(entries) = custom {
        chapter_path(
            entries,
            at,
            false,
            |entry| {
                let pos = &entry.position;
                match pos.anchor.as_ref() {
                    Some(a) => Some((pos.chapter, a.block, a.offset)),
                    None if pos.chapter != hit.chapter || pos.page == 0 => {
                        Some((pos.chapter, 0, 0))
                    }
                    None => None,
                }
            },
            |entry| (0, entry.label.as_str()),
        )
    } else {
        chapter_path(&probe.toc, at, false, key, |entry| {
            (entry.depth, entry.label.as_str())
        })
    };
    if titles.is_empty() && !hit.chapter_title.is_empty() {
        titles.push(hit.chapter_title.clone());
    }
    Ok(ResultContext::new(label, titles))
}

impl Reader {
    pub(super) fn draw_match_rows(&self, canvas: &mut Canvas, group: &Rows<'_>) {
        use primitive_ui::{HAIRLINE, SECONDARY, chevron};

        if group.rows.is_empty() {
            return;
        }
        canvas.card(group.bounds);
        let first = self.list_page * self.match_geometry().rows;
        for (offset, detail) in group.rows.iter().enumerate() {
            let Some(row) = group.row_rect(offset) else {
                continue;
            };
            let index = first + offset;
            let hit = if self.fixed.is_some() {
                self.pdf_hits
                    .get(index)
                    .map(|hit| (&hit.snippet, &hit.matched, false))
            } else {
                self.hits
                    .get(index)
                    .map(|hit| (&hit.snippet, &hit.matched, true))
            };
            let Some((text, range, expand)) = hit else {
                continue;
            };
            let pressed = canvas.control_pressed(row, false);
            if pressed {
                canvas.panel(row.inset(4), 16, 2, 0);
            }
            let ink = if pressed { 255 } else { 20 };
            let detail_ink = if pressed { 255 - SECONDARY } else { SECONDARY };
            let left = row.x + 32;
            let right = row.right() - 84;
            let available = right - left;
            let stacked = row.h >= 102;
            let detail_width = if stacked {
                available
            } else {
                available * 24 / 100
            };
            let title_width = if stacked {
                available
            } else {
                available - detail_width - 12
            };
            let detail_size = (row.h / 3).clamp(20, 28);
            let detail_height = detail_size * 13 / 10 + 3;
            let tall = if stacked {
                row.h - detail_height - 14
            } else {
                row.h - 10
            };
            let title_size = if (73..100).contains(&tall) {
                (tall / 2 - 1) * 10 / 13
            } else {
                (tall * 10 / 13).min(38)
            };
            let line_height = (title_size * 13).div_euclid(10) + 1;
            let lines = (tall / line_height).clamp(1, 3) as usize;
            // Dense rows retain the match, even in a run of wide characters.
            // Taller rows can use more of the saved context on either side.
            let prefix = if lines > 1 { 52 } else { 20 };
            let (shown, emphasis) = row_excerpt(text, range.clone(), expand, prefix);
            let rendered_lines = if lines > 1
                && canvas.measure_with_weight(&shown, title_size, true) > title_width
            {
                lines as i32
            } else {
                1
            };
            let title_height = (rendered_lines * line_height + 2).min(tall);
            let stack_height = title_height + detail_height + 6;
            let title_y = row.y
                + if stacked {
                    (row.h - stack_height) / 2
                } else {
                    (row.h - title_height) / 2
                };
            canvas.emphasized_text(
                Rect::new(left, title_y, title_width, title_height),
                &shown,
                TextStyle::new(title_size).ink(ink).lines(lines),
                emphasis,
            );
            if let Some(detail) = detail.detail {
                let detail_y = if stacked {
                    title_y + title_height + 6
                } else {
                    row.y + (row.h - detail_height) / 2
                };
                let detail_x = if stacked { left } else { right - detail_width };
                canvas.text(
                    Rect::new(detail_x, detail_y, detail_width, detail_height),
                    detail,
                    TextStyle::new(detail_size).ink(detail_ink),
                );
            }
            let size = (row.h - 10).min(40);
            chevron(
                canvas,
                Rect::new(
                    row.right() - 32 - size,
                    row.y + (row.h - size) / 2,
                    size,
                    size,
                ),
                ink,
            );
            if self.focused_result == Some(index) {
                canvas.fill(Rect::new(row.x + 10, row.y + 8, 4, row.h - 16), ink);
            }
            if offset + 1 < group.rows.len() {
                canvas.fill(Rect::new(left, row.bottom() - 2, row.w - 64, 2), HAIRLINE);
            }
        }
    }

    pub(super) fn preview_result(&mut self, index: usize) -> Result<()> {
        let context = if let Some(fixed) = &self.fixed {
            let page = self
                .pdf_hits
                .get(index)
                .ok_or("That search result is unavailable")?
                .page;
            let titles = if self.custom_contents_enabled() {
                let state = self
                    .library
                    .state(fixed.id)
                    .ok_or("Reading record unavailable")?;
                chapter_path(
                    state.custom_contents.entries(),
                    page,
                    true,
                    |entry| Some(entry.position.page),
                    |entry| (0, entry.label.as_str()),
                )
            } else {
                chapter_path(
                    fixed.reading.outline(),
                    page,
                    true,
                    |entry| entry.page,
                    |entry| (entry.depth, entry.title.as_str()),
                )
            };
            ResultContext::new(fixed_page_label(&fixed.reading, page), titles)
        } else {
            let active = self.active.as_ref().ok_or("Open a book first")?;
            let hit = self
                .hits
                .get(index)
                .ok_or("That search result is unavailable")?;
            let custom = self
                .custom_contents_enabled()
                .then(|| {
                    self.library
                        .state(active.id)
                        .map(|state| state.custom_contents.entries())
                })
                .flatten();
            reflow_context(active, hit, custom)?
        };
        self.focused_result = Some(index);
        self.result_context = Some(context);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};
    use std::thread;

    fn harbour_pdf() -> Vec<u8> {
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R /Outlines 8 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 /Resources << /Font << /F 7 0 R >> >> >>".into(),
        ];
        for i in 0..4 {
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 390 550] /Contents {} 0 R >>",
                12 + i
            ));
        }
        objects.extend([
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
            "<< /Type /Outlines /First 9 0 R /Last 9 0 R /Count 3 >>",
            "<< /Title (Coastal stations) /Parent 8 0 R /First 10 0 R /Last 11 0 R /Count 2 /Dest [3 0 R /Fit] >>",
            "<< /Title (North inlet) /Parent 9 0 R /Next 11 0 R /Dest [3 0 R /Fit] >>",
            "<< /Title (Outer harbour) /Parent 9 0 R /Prev 10 0 R /Dest [5 0 R /Fit] >>",
        ].map(str::to_owned));
        let entries = [
            "The morning signal travels between the piers.",
            "The second signal reaches the channel.",
            "A clear signal marks the outer turn.",
            "The evening signal crosses the water.",
            "A final signal closes the log.",
        ];
        for i in 0..4 {
            let mut stream = format!(
                "BT /F 18 Tf 20 500 Td (Coastal stations - log {}) Tj 0 -34 Td /F 12 Tf",
                i + 1
            );
            for entry in entries {
                stream.push_str(&format!(" ({entry}) Tj 0 -24 Td"));
            }
            stream.push_str(" ET\n");
            objects.push(format!(
                "<< /Length {} >>\nstream\n{stream}endstream",
                stream.len()
            ));
        }
        crate::fixed::tests::pdf_objects(&objects)
    }

    #[test]
    fn matching_style_keeps_entire_reflow_words_and_original_unicode_clusters() {
        let text = "The whole café remains.";
        let start = text.find("afé").unwrap();
        let (shown, emphasis) = row_excerpt(text, start..start + "afé".len(), true, 16);
        assert_eq!(&shown[emphasis], "café");
        let (shown, emphasis) = row_excerpt(text, start..start + "afé".len(), false, 16);
        assert_eq!(&shown[emphasis], "afé");
        let s = format!("{} علامة رصيف", "水\u{301}".repeat(32));
        let i = s.find("علامة").unwrap();
        let (shown, range) = row_excerpt(&s, i..i + "علامة".len(), true, 16);
        assert_eq!(&shown[range], "علامة");
        assert!(shown.starts_with('…'));
        assert!(!shown.starts_with("…\u{301}"));
    }

    #[test]
    fn search_context_resolves_nested_headings_first_on_same_fixed_page_and_last_prior() {
        let rows = [
            ("Coast", 0, Some(0)),
            ("Inlet", 1, Some(1)),
            ("Channel", 2, Some(1)),
            ("Arrival", 1, Some(3)),
        ];
        let labels = |page| chapter_path(&rows, page, true, |v| v.2, |v| (v.1, v.0));
        assert_eq!(labels(1), ["Coast", "Inlet"]);
        assert_eq!(labels(2), ["Coast", "Inlet", "Channel"]);
        assert_eq!(labels(3), ["Coast", "Arrival"]);
    }

    #[test]
    fn result_hold_displays_context_keeps_the_row_and_tap_opens_its_passage() {
        let fixture = Fixture::new();
        let external = std::env::var_os("READER_TEST_CONTEXT_DOCUMENT").map(PathBuf::from);
        let authored = external.is_none();
        let mut fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        if let Some(bold) = std::env::var_os("READER_TEST_CONTEXT_BOLD_FONT") {
            fonts.push(bold.into());
        }
        let cases = if let Some(path) = external {
            vec![(path, std::env::var("READER_TEST_CONTEXT_QUERY").unwrap())]
        } else {
            let pdf = fixture.root.join("Coastal stations.pdf");
            fs::write(&pdf, harbour_pdf()).unwrap();
            let epub = fixture.root.join("Harbour notebook.epub");
            fs::write(
                &epub,
                include_bytes!("../../tests/fixtures/many-chapters.epub"),
            )
            .unwrap();
            vec![(pdf, "ign".into()), (epub, "arbo".into())]
        };
        for (path, query) in cases {
            let mut app =
                Reader::open(path.parent().unwrap(), &fixture.state, fonts.clone()).unwrap();
            app.open_path(&path).unwrap();
            let fixed = app.fixed.is_some();
            let origin = app.current_history_location().unwrap();
            app.start_document_search(&query).unwrap();
            let external_index = std::env::var("READER_TEST_CONTEXT_INDEX")
                .ok()
                .map(|s| s.parse::<usize>().unwrap());
            let available = |app: &Reader| {
                if fixed {
                    app.pdf_hits.len()
                } else {
                    app.hits.len()
                }
            };
            let deadline = Instant::now() + Duration::from_secs(420);
            while Instant::now() < deadline
                && (app.search_at.is_some() || app.nearby.searching_all())
            {
                if fixed {
                    app.step_pdf_search(Instant::now()).unwrap();
                } else {
                    app.poll_nearby(Instant::now()).unwrap();
                    thread::sleep(Duration::from_millis(2));
                }
                if external_index.is_some_and(|i| available(&app) > i) {
                    break;
                }
            }
            let count = available(&app);
            let index = external_index.unwrap_or(count - 1);
            let geometry = app.match_geometry();
            assert!(index >= geometry.rows && index < count);
            while app.list_page < index / geometry.rows {
                assert_eq!(app.input(Input::PageForward), Change::Partial);
            }
            if authored && fixed {
                capture_test_screen(&app, &fonts, "READER_CONTEXT_LIST_SCREENSHOT", false);
            }
            let x = (LIST_X + 72) as f32;
            let row = geometry.row(index % geometry.rows);
            let y = (row.y + row.h / 2) as f32;
            let preview_start = Instant::now();
            assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
            let hold_ms = preview_start.elapsed().as_millis();
            let context = app
                .result_context
                .as_ref()
                .unwrap_or_else(|| panic!("Preview failed: {:?}", app.notice));
            assert!(!context.label.is_empty());
            if authored && fixed {
                assert_eq!(context.label, "Page 4");
                assert_eq!(context.chapters, ["Coastal stations", "Outer harbour"]);
                capture_test_screen(&app, &fonts, "READER_CONTEXT_HOLD_SCREENSHOT", false);
            }
            if authored && !fixed {
                assert!(context.label.starts_with("Chapter 48 of 48 · "));
                assert_eq!(context.chapters, ["Day 48"]);
            }
            let (excerpt, range) = if fixed {
                let hit = &app.pdf_hits[index];
                row_excerpt(&hit.snippet, hit.matched.clone(), false, 16)
            } else {
                let hit = &app.hits[index];
                row_excerpt(&hit.snippet, hit.matched.clone(), true, 16)
            };
            let emphasized_word = format!("{:x}", md5::compute(excerpt[range].as_bytes()));
            let label = context.label.clone();
            let headings = context.chapters.clone();
            assert_eq!(app.current_history_location().unwrap(), origin);
            assert_eq!(app.input(Input::PageBack), Change::Partial);
            assert!(app.result_context.is_none());
            assert_eq!(app.list_page, index / geometry.rows);
            assert_eq!(app.focused_result, Some(index));
            assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
            assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
            assert!(app.result_context.is_none());
            assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
            assert_eq!(app.search_result, Some(index));
            if fixed {
                assert_eq!(
                    app.fixed.as_ref().unwrap().reading.page(),
                    app.pdf_hits[index].page
                );
                assert_eq!(
                    app.pdf_result_regions.len(),
                    app.pdf_hits[index].bounds.len()
                );
            } else {
                assert_eq!(
                    app.active.as_ref().unwrap().reading.chapter,
                    app.hits[index].chapter
                );
                assert_eq!(
                    app.result_selection.as_ref().unwrap().start,
                    app.hits[index].anchor
                );
            }
            app.activate(Action::ReturnToLocation).unwrap();
            assert_eq!(
                app.current_history_location().unwrap().position,
                origin.position
            );
            if let Some(out) = std::env::var_os("READER_TEST_CONTEXT_REPORT") {
                let sums: Vec<_> = headings
                    .iter()
                    .map(|s| format!("{:x}", md5::compute(s.as_bytes())))
                    .collect();
                let report = serde_json::json!({
                    "index": index,
                    "hold_ms": hold_ms,
                    "emphasized_word": emphasized_word,
                    "labels": sums,
                    "page_label": format!("{:x}", md5::compute(label.as_bytes())),
                    "source": if fixed { "pdf" } else { "epub" },
                    "restored": true,
                });
                fs::write(out, serde_json::to_vec(&report).unwrap()).unwrap();
            }
        }
    }
}
