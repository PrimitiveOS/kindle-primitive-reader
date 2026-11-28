# EPUB cross-spine in-page notes: integration plan

Status: planned, not implemented. The pinned KOReader draws the note band on
the referencing page for a note in another spine item (see
`docs/screenshots/in-page-cross-spine-koreader-page00.png`). Reader draws no
band (`docs/screenshots/in-page-cross-spine-reader-page00.png`). The fixture is
`tests/fixtures/in-page-cross-spine.epub`.

## What exists for FictionBook 3

- `src/footnotes/cross_part.rs` `prepare_for_settings`: scans the host
  chapter's explicit links, resolves each to another part, and accepts it only
  when `book.complete_fb3_notes()` maps the note id to that part. It is bounded
  (`MAX_REFERENCES`, `MAX_NOTES`, `MAX_CHAPTER_BYTES`).
- `Layout` builder: lays out the foreign note with
  `Targets::detached_fb3` (`src/footnotes/in_page.rs`, requires an `aside`
  root and FB2 note scale), `BookStyle::load_with_base`, and
  `compute_deferred_quotes`. It refuses publisher signed margins, web fit
  content, hanging punctuation, and mismatched language or hyphenation.
- Session side: `foreign_band`, `foreign_band_at`, `foreign_note_bands`
  (`src/lib.rs`), with hit testing restricted to text columns.

## What EPUB needs

1. Target resolution. Replace the `complete_fb3_notes` manifest check with a
   check that the link's path is another reading-order document in the same
   book and that the fragment names an `aside` or note-typed element there
   (`epub:type` footnote, endnote, note, or the publisher's note classes
   already recognised by `Targets::epub_for_settings`).
2. Detached note targets. Add an EPUB counterpart of `detached_fb3` that uses
   the EPUB note scale (`Targets::epub_for_settings`, `NoteSize::Epub`) and
   the existing `FootnoteSize` user setting.
3. Layout of the foreign document. The foreign note's chapter must be
   prepared with the same settings and styles as the host, so its
   `ParleyChapter` lines and source keys match what `Layout` expects.
4. Session wiring. `Reading` already keeps foreign bands for FB3; check that
   the same paint path, selection, and link hit-testing work when the host is
   an EPUB spine item and the foreign source is another spine document.
5. Gates to keep: the in-page composer keeps its own rules (single page,
   no facing, no scroll, no hidden flows), and the foreign band never covers
   the host's own note flow.

## Tests to add first

- A probe test on `tests/fixtures/in-page-cross-spine.epub` that expects one
  foreign band on the referencing page, with the note text from `notes.xhtml`.
- A selection test: a word in the foreign band selects the note text and
  its link back to `chapter.xhtml#r1`.
- A regression test that a same-chapter note keeps the existing in-page
  composer output unchanged.

## Verification

Re-run `scripts/koreader-scroll-inpage-footnotes-check.lua` on the fixture
and compare the band position and separator with the pinned KOReader frame.
Record the result in `docs/koreader-gap-register.md` under S-15.
