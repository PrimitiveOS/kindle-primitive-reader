//! Preview annotations stored beside the open EPUB before merging them with Reader data.

use super::*;
use crate::koreader_sidecar::SidecarMark;
use crate::library::{BackupSummary, merge_text_marks};

pub(super) struct MarkImport {
    book_id: u64,
    source_digest: [u8; 16],
    version: Option<u32>,
    entries: Vec<SidecarMark>,
    next: usize,
    unavailable: usize,
    bookmarks: Vec<Bookmark>,
    highlights: Vec<Highlight>,
    pub(super) deadline: Option<Instant>,
}

fn count_label(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

impl MarkImport {
    pub(super) fn items(&self) -> Vec<Item> {
        let matched = format!(
            "{} · {} · {} unavailable",
            count_label(self.bookmarks.len(), "bookmark"),
            count_label(self.highlights.len(), "highlight"),
            self.unavailable
        );
        if self.deadline.is_some() {
            return vec![
                Item::new(
                    format!("Checking {} of {}", self.next, self.entries.len()),
                    Action::Heading,
                )
                .detail(matched),
            ];
        }
        let mut items = vec![Item::new("Saved beside this book", Action::Heading).detail(matched)];
        if !self.highlights.is_empty() || !self.bookmarks.is_empty() {
            items.insert(0, Item::new("Add matched marks", Action::ApplyNearbyMarks));
        }
        items.extend(
            self.bookmarks
                .iter()
                .map(|m| Item::new(&m.label, Action::Heading).detail("Bookmark")),
        );
        items.extend(self.highlights.iter().map(|m| {
            Item::new(&m.selection.text, Action::Heading)
                .detail(m.note.as_deref().unwrap_or("Highlight"))
        }));
        items
    }
}

enum Resolved {
    Point(crate::epub::EpubPosition),
    Range(crate::epub::EpubPosition, crate::epub::EpubPosition),
}

impl Reader {
    fn nearby_mark_source(&self) -> Result<(u64, Vec<u8>, SidecarProgress)> {
        let active = self.active.as_ref().ok_or("Open an EPUB first")?;
        if active.entry.format != DocumentFormat::Epub {
            return Err("Open an EPUB first".into());
        }
        let (path, checksum) = with_storage(|| {
            if self.library.identify(&active.entry)? != active.id {
                return Err(io::Error::other("Reopen the changed book first"));
            }
            let path = self
                .library
                .nearby_koreader_metadata(&active.entry.path)?
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "No KOReader marks were found beside this book",
                    )
                })?;
            let mut file = fs::File::open(self.library.absolute_path(&active.entry.path)?)?;
            Ok((path, koreader_sidecar::partial_md5(&mut file)?))
        })?;
        let bytes = read_import_file(&path, koreader_sidecar::MAX_SIDECAR_BYTES as u64)?;
        let progress = koreader_sidecar::parse(&bytes)?;
        progress.check_fingerprint(checksum)?;
        Ok((active.id, bytes, progress))
    }

    pub(super) fn preview_nearby_marks(&mut self) -> Result<()> {
        let (book_id, bytes, progress) = self.nearby_mark_source()?;
        let source = koreader_sidecar::parse_annotations(&bytes)?;
        self.show(Screen::ImportAnnotations);
        self.annotation_import = None;
        self.nearby_marks = Some(MarkImport {
            book_id,
            source_digest: md5::compute(&bytes).0,
            version: progress.cre_dom_version,
            deadline: (!source.marks.is_empty()).then(Instant::now),
            entries: source.marks,
            next: 0,
            unavailable: source.unavailable,
            bookmarks: Vec::new(),
            highlights: Vec::new(),
        });
        Ok(())
    }

    pub(super) fn step_nearby_marks(&mut self) -> Result<Change> {
        let active = self.active.as_ref().ok_or("Open the same EPUB first")?;
        let import = self
            .nearby_marks
            .as_mut()
            .ok_or("Preview this book’s marks first")?;
        let Some(entry) = import.entries.get(import.next) else {
            return Ok(Change::None);
        };
        let mapped = with_storage(|| {
            let mut book = Book::open(&active.reading.config.book).map_err(io::Error::other)?;
            let mut resolve =
                |pointer: &str| crate::cre_pointer::resolve(&mut book, pointer, import.version);
            // A changed or unsupported location skips only this record.
            Ok(match entry {
                SidecarMark::Bookmark { pointer, .. } => resolve(pointer).map(Resolved::Point),
                SidecarMark::Highlight { start, end, .. } => {
                    resolve(start).and_then(|a| resolve(end).map(|b| Resolved::Range(a, b)))
                }
            })
        })?;
        let added = match (entry, mapped) {
            (
                SidecarMark::Bookmark {
                    label,
                    note,
                    created_at,
                    ..
                },
                Ok(Resolved::Point(p)),
            ) => {
                let label = label.clone().unwrap_or_else(|| {
                    active.reading.probe.chapter_entries()[p.chapter]
                        .label
                        .clone()
                });
                import.bookmarks.push(Bookmark {
                    note: note.clone(),
                    updated_at: None,
                    chapter_title: None,
                    created_at: *created_at,
                    label,
                    position: ReadingPosition {
                        chapter: p.chapter,
                        page: 0,
                        anchor: Some(p.anchor),
                    },
                });
                true
            }
            (
                SidecarMark::Highlight {
                    text,
                    note,
                    style,
                    color,
                    created_at,
                    ..
                },
                Ok(Resolved::Range(a, b)),
            ) => {
                let selection = active
                    .reading
                    .anchored_quote(&a.anchor, &b.anchor)
                    .ok()
                    .filter(|quote| quote.text.split_whitespace().eq(text.split_whitespace()));
                if let Some(selection) = selection {
                    import.highlights.push(Highlight {
                        updated_at: None,
                        chapter_title: None,
                        color: *color,
                        created_at: *created_at,
                        style: *style,
                        selection,
                        note: note.clone(),
                    });
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if !added {
            import.unavailable += 1;
        }
        import.next += 1;
        import.deadline =
            (import.next < import.entries.len()).then(|| Instant::now() + SEARCH_DELAY);
        Ok(Change::Partial)
    }

    pub(super) fn apply_nearby_marks(&mut self) -> Result<()> {
        let (id, bytes, _) = self.nearby_mark_source()?;
        let import = self
            .nearby_marks
            .as_ref()
            .ok_or("Preview this book’s marks first")?;
        if import.deadline.is_some()
            || id != import.book_id
            || md5::compute(&bytes).0 != import.source_digest
        {
            return Err("The preview is incomplete or changed. Open it again.".into());
        }
        let active = self.active.as_ref().ok_or("Open the same EPUB first")?;
        let state = self
            .library
            .remember(&active.entry, id, &active.reading.probe.title);
        let mut summary = BackupSummary::default();
        merge_text_marks(state, &import.bookmarks, &import.highlights, &mut summary);
        if summary.bookmarks_added
            + summary.highlights_added
            + summary.notes_added
            + summary.timestamps_added
            > 0
        {
            self.mark_dirty();
            self.save()?;
        }
        self.show(Screen::Highlights);
        let mut notice = format!(
            "Added {} and {} · {} existing",
            count_label(summary.bookmarks_added, "bookmark"),
            count_label(summary.highlights_added, "highlight"),
            summary.duplicates
        );
        if summary.notes_added > 0 {
            notice.push_str(&format!(
                " · {} saved",
                count_label(summary.notes_added, "note")
            ));
        }
        if summary.note_conflicts > 0 {
            notice.push_str(&format!(
                " · {} kept",
                count_label(summary.note_conflicts, "existing note")
            ));
        }
        if summary.skipped > 0 {
            notice.push_str(&format!(" · {} skipped", summary.skipped));
        }
        self.notice = Some(notice);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Fixture, capture_test_screen, tap_contents_item};
    use super::*;
    #[test]
    fn nearby_bookmarks_and_highlights_match_the_source_and_merge_only_once() {
        let fixture = Fixture::new();
        let supplied = std::env::var_os("READER_TEST_NEARBY_MARKS").map(PathBuf::from);
        let book = supplied.as_ref().unwrap_or(&fixture.book);
        if supplied.is_none() {
            let dir = book.with_extension("sdr");
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("metadata.epub.lua"), br#"return {
                cre_dom_version=20240114,partial_md5_checksum="71bcf61d42cc5f9e83a2bceb144ad66f",
                annotations={
                    {page="/body/DocFragment[2]/body/p[1]/text().4",text="Evening",datetime="2024-01-01T00:00:00Z"},
                    {drawer="lighten",pos0="/body/DocFragment[1]/body/p[9]/text().4",
                     pos1="/body/DocFragment[1]/body/p[9]/text().8",text="tide",note="A repeated word",datetime="2024-01-02 18:32:07",datetime_updated="2026-02-03 04:05:06"},
                    {drawer="lighten",pos0="/body/DocFragment[1]/body/p[9]/text().4",
                     pos1="/body/DocFragment[1]/body/p[9]/text().8",text="different"},
                } }"#).unwrap();
        }
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let directory = book.parent().unwrap();
        let mut app = Reader::open(directory, &fixture.state, fonts.clone()).unwrap();
        app.open_path(book).unwrap();
        let held = app.current_position().unwrap();
        let active = app.active.as_ref().unwrap();
        let id = active.id;
        let metadata = app
            .library
            .nearby_koreader_metadata(&active.entry.path)
            .unwrap()
            .unwrap();
        let original = fs::read(&metadata).unwrap();
        let source = koreader_sidecar::parse_annotations(&original).unwrap();
        let mut counts = (0, 0, 0);
        for run in 0..2 {
            app.show(Screen::Highlights);
            tap_contents_item(&mut app, "Import marks saved beside this book");
            while app.nearby_marks.as_ref().unwrap().deadline.is_some() {
                app.step_nearby_marks().unwrap();
            }
            let import = app.nearby_marks.as_ref().unwrap();
            counts = (
                import.bookmarks.len(),
                import.highlights.len(),
                import.unavailable,
            );
            if supplied.is_some() {
                assert_eq!(counts.2, 0);
                assert_eq!(counts.0 + counts.1, source.marks.len());
                assert!(counts.0 + counts.1 > 0);
            } else {
                assert_eq!(counts, (1, 1, 1));
                assert_eq!(import.highlights[0].selection.text, "tide");
                assert_eq!(
                    import.highlights[0].note.as_deref(),
                    Some("A repeated word")
                );
                if run == 0 {
                    fs::write(&metadata, [original.as_slice(), b"\n"].concat()).unwrap();
                    assert!(app.activate(Action::ApplyNearbyMarks).is_err());
                    fs::write(&metadata, &original).unwrap();
                }
            }
            if run == 0 {
                assert!(app.library.state(id).unwrap().highlights.is_empty());
                assert!(app.library.state(id).unwrap().bookmarks.is_empty());
                if supplied.is_none() {
                    capture_test_screen(&app, &fonts, "READER_NEARBY_MARKS_SCREENSHOT", false);
                }
            }
            assert_eq!(app.current_position(), Some(held.clone()));
            tap_contents_item(&mut app, "Add matched marks");
            assert_eq!(app.current_position(), Some(held.clone()));
        }
        assert_eq!(fs::read(metadata).unwrap(), original);
        drop(app);
        let app = Reader::open(directory, &fixture.state, fonts).unwrap();
        let saved = app.library.state(id).unwrap();
        assert_eq!(
            (saved.bookmarks.len(), saved.highlights.len()),
            (counts.0, counts.1)
        );
        if supplied.is_none() {
            assert_eq!(saved.bookmarks[0].created_at, Some(1_704_067_200));
            assert_eq!(
                saved.highlights[0].created_at,
                crate::mark_time::parse(b"2024-01-02 18:32:07")
            );
        }
        let expected_bookmark_times = source.marks.iter().filter_map(|mark| match mark {
            SidecarMark::Bookmark { created_at, .. } => Some(*created_at),
            _ => None,
        });
        assert!(
            saved
                .bookmarks
                .iter()
                .map(|m| m.created_at)
                .eq(expected_bookmark_times)
        );
        for (mark, (expected, created_at)) in
            saved
                .highlights
                .iter()
                .zip(source.marks.iter().filter_map(|mark| match mark {
                    SidecarMark::Highlight {
                        text, created_at, ..
                    } => Some((text, created_at)),
                    _ => None,
                }))
        {
            assert_eq!(mark.created_at, *created_at);
            assert!(
                mark.selection
                    .text
                    .split_whitespace()
                    .eq(expected.split_whitespace())
            );
        }
    }
}
