# Cross-part FB3 in-page notes

## Current behavior

Reader can copy one complete text-only FB3 note from a different converted
chapter into the reference page's bottom band. It requires exactly one eligible
reference and note, and it verifies that the entire band and separator cover
only blank pixels on the original page. The original link, canonical note and
host reading position remain available. Interactions inside a displayed note
resolve against its original chapter: this includes links, selections, custom
contents and page translation. A note whose own body spans converted
chapters, a crowded page, multiple references, or unsupported content keeps
the ordinary link flow. [FB3 reading](fb3-reading.md#in-page-footnotes)

The bounded import inventory records up to 512 complete, unique note IDs and
their existing converted chapter paths. It excludes split, nested, duplicate
and discarded note bodies. This inventory is metadata and does not change
converted XHTML or source revisions. The candidate resolver retains each
selected note's real chapter, Source revision and block keys. It accepts only
explicit references to supported text-only notes, with bounded references,
chapter markup, DOM size, note bytes, elements and blocks. The detached layout
checks the effective CSS, source identity and page geometry before displaying
a copy. It rejects generated or positioned content, hidden text, vertical text,
images, tables, lists and ruby rather than displaying a partial note. Notes
with more than 128 links also stay in ordinary flow so every displayed link
remains discoverable. The foreign text uses the current book-language and
embedded-language preferences. Signed horizontal margins, Web content fitting
and hanging punctuation currently fall back to ordinary links because they can
place selectable text beyond the overlay's hit-testing columns. The separator
uses the same right-to-left majority and left-aligned tie policy as local notes.
[Importer](../src/fb3/body.rs) · [Candidate resolver](../src/footnotes/cross_part.rs)

## Native comparison

The pinned KOReader opens the unchanged `harbor-continuation.fb3` as one
document. Its early reference displays the later note in a bottom band with
the FictionBook footnote setting. At 720 × 480, the separator appears at
y=411 with the default 75% note size and y=400 at regular size. The original
note and backlink remain on page 1291. Turning the setting off removes the
band. The separate EPUB and FictionBook comments settings do not enable it.
[Fixture](../tests/fixtures/harbor-continuation.fb3) ·
[Native replay](../scripts/koreader-in-page-footnotes-check.lua) ·
[Off](screenshots/stock-fb3-cross-note-off.png) ·
[75%](screenshots/stock-fb3-cross-note-on.png) ·
[Regular](screenshots/stock-fb3-cross-note-regular.png) ·
[Original note](screenshots/stock-fb3-cross-note-original.png)

The additional `cross-note-quiet.fb3` fixture puts the short reference in the
first converted chapter and the complete note in the third. The intervening
section forces both boundaries while leaving the reference page's lower area
blank. Pinned KOReader places the separator at y=411 at 75% and y=360 at
regular size; the original note and backlink remain on page 830.
[Quiet fixture](../tests/fixtures/cross-note-quiet.fb3) ·
[KOReader off](screenshots/stock-fb3-cross-quiet-note-off.png) ·
[KOReader 75%](screenshots/stock-fb3-cross-quiet-note-on.png) ·
[KOReader regular](screenshots/stock-fb3-cross-quiet-note-regular.png) ·
[KOReader original](screenshots/stock-fb3-cross-quiet-note-original.png)

The Reader simulation shows the same note and backlink at the bottom, with a
short left-aligned separator. Its separator is approximately four pixels lower
in this fixture, and its reference is full-sized and underlined while KOReader
renders the reference smaller. These captures establish the feature's current
behavior; they do not establish pixel-level parity.
[Reader off](screenshots/reader-fb3-cross-quiet-note-off.png) ·
[Reader 75%](screenshots/reader-fb3-cross-quiet-note-on.png) ·
[Reader regular](screenshots/reader-fb3-cross-quiet-note-regular.png)

## Remaining composition work

1. Validate the supported band on the Kindle, including reflow, sleep and USB
   lifecycle. Keep the reference page and canonical note positions intact.
2. Support repeated references and notes that need repagination without
   overlapping the host body. Add explicit presentation anchors before allowing
   note-only continuation pages.
3. Handle notes whose bodies themselves span parts using out-of-band
   continuation spans, preserving each part's canonical Source. Cross-source
   selection, images, tables, ruby and generated text each need their own
   rendering, selection, restoration and budget checks.

The existing limits of 32 notes, 128 references, 512 pages and 16,384 lines
still apply. Any missing identity, unsupported content or failed budget check
must retain ordinary flow rather than showing a partial note.
