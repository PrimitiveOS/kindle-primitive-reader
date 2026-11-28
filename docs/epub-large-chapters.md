# Markup-heavy EPUB chapters

Reflow EPUB spine XHTML and HTML can now use up to 1 MiB of encoded member
bytes and 1 MiB of decoded or recovered XML. This helps with markup-heavy
chapters whose reading text still fits the existing 512 KiB text limit. The
XML parser's node limit and the existing image, font and layout budgets still
apply. Package and container metadata, navigation documents and fixed-layout
EPUB XML retain their 512 KiB limit; static SVG has its separate smaller limit.
Text-heavy chapters above the ordinary layout text limit can use the bounded
windowed Parley route when they are packaged EPUBs whose computed layout fits
the supported stream rules. Other large chapters still fail clearly instead
of being rewritten into synthetic spine entries.

The chapter's original archive path, content, source revision, spine index and
saved anchors are unchanged for already readable chapters. Normal reading,
search, CFI and KOReader-pointer paths obtain reflow content through the same
bounded chapter reader. Navigation group-target inference retains its 512 KiB
per-chapter inspection limit; it may leave a grouping row unavailable for a
larger chapter. Progress weighting still caps each chapter's contribution at
512 KiB, so it can underweight larger chapters. Optional eager preloading,
quote-prefix preparation and cross-part note discovery retain their independent
budgets and can defer work, fall back or decline the operation. The KOReader
pointer import's 512 KiB budget applies to linked stylesheets, not chapter text.

The larger memory bound and windowed route are implementation candidates, not
device acceptance results. Decoder expansion, peak memory, real EPUBs, simulator behavior and
Kindle performance remain unverified.

## Windowed long-chapter reading

The windowed Parley path batches the original EPUB chapter's canonical text
blocks without creating replacement chapters or changing saved positions.
It keeps the original book/chapter identity, source revision, block keys and
UTF-8 offsets. A snapshot-bound index computes page bounds; the reading
session verifies those bounds against page replay and retains only the source
batches needed for the requested page.

The route remains deliberately narrow: at most 1 MiB of original chapter and
canonical text, 8,192 blocks/elements, 128 KiB or 256 blocks per batch, 128 KiB
per block and 32 batches. It accepts publisher stylesheets and inline text
styles from the captured EPUB when their computed styles fit the stream's
geometry rules. CSS-only user styles are captured with the layout and accepted
only when their computed text styles pass those same geometry, source-flow and
font checks; paint-only visibility masks preserve source flow, while suppressed
box flow, separate layout tweaks and note modes remain excluded. It rejects
nonunique or unsupported chapters, images, tables,
lists, ruby, raw whitespace, generated content, special breaks, complex boxes,
non-horizontal layouts and unsupported reading modes. The original book,
spine and saved state remain unchanged.

The session captures the packaged EPUB and configured font stack, checks file
stamps around capture, and builds the source, computed styles and page index
from those immutable bytes. It then checks the live chapter against the
snapshot and rejects the handoff if the page bounds differ from the reading
layout. The supported route still does not prove equivalence with
system-discovered fallback fonts.

The incremental paginator carries page origin and line overlap across work
batches, using the ordinary paginator for layouts without publisher breaks,
split spaces or fitted tails.
It checks original block keys and UTF-8 boundaries, limits the stream to 30,000
lines and 16,384 pages, and returns no partial bounds on cancellation or
failure. It also applies Parley's unpainted-tail trim using only painted
glyphs and decorations, so a trailing visibility-hidden paragraph does not
extend the painted boundary. Batch boundaries never create page boundaries.
The active path compares the resulting bounds with the windowed layout before
accepting the result.

A separate internal simple-Parley stream now shapes one planned block batch at
a time while carrying the original layout's vertical position, row-flow state,
paragraph gap and paint extent. It yields borrowed paragraph layouts and
original-source line ranges to a callback; the batch is dropped afterward.
The stream repeats the source and conservative style gates, rejects coupled
layout modes and inline direction overrides, and limits each individual
paragraph to 16 KiB, the chapter to 30,000 lines and each work batch to the
planner's 128 KiB. Cancellation is checked between paragraphs and before and
after delivery; one shaping operation is not interruptible. A consumer must
stage and discard its output if any later batch fails or is cancelled.

The simple stream now feeds a narrow `Probe` path for eligible long chapters.
The session captures a bounded packaged EPUB and configured font stack, checks
their file stamps around capture, and supplies those immutable bytes to the
index builder. The index then verifies that its chapter, XHTML, source revision,
block keys and text match the live `Probe` source. Pagination must produce the
same page-bound vector as the index or the layout fails closed. Page painting
replays only the batches intersecting the requested page, verifies each replay
transcript, and retains those paragraphs as the current window. Adjacent page
captures reuse that verified batch window while it contains every batch needed
by the requested page, avoiding repeated shaping across page turns within one
batch range. The window remains bounded to the currently retained replay
range; moving outside it replaces the cached paragraphs. The session does not
attach the ordinary raw-layout cache to this windowed layout. It can attach
the exact screen-page source stamp only after the live page boundaries match
the snapshot index.

This is an integrated but unverified candidate, not a parity claim. It is
currently limited to packaged reflowable EPUB chapters over the existing
512 KiB chapter threshold, Parley, supported text-focused horizontal styles,
no embedded fonts, and no scroll, facing-page, hidden-flow, nonlinear-hiding
or in-page-note mode. CSS-only user styles are accepted only when the snapshot
contains no separate layout tweak or note mode and their computed styles pass
the stream's conservative geometry and source-flow gates. Paint-only visibility
masks preserve layout metrics and suppress glyph painting; suppressed box flow
remains unsupported.
This CSS path is an unverified implementation candidate. It snapshots
configured font files only; system discovery and fallback-font equivalence are
not proved.
Layout and page replay are synchronous in the current `Probe` path. Background
long-chapter snapshot reads, configured-font capture, and index construction
now receive the worker's cancellation flag; a ZIP parse, font registration,
or single paragraph shape is still not interruptible. Cancellation latency
and Kindle responsiveness have not been measured. Search, selection,
navigation and saved-position behavior have not been checked against the
windowed source. There are no behavioral, simulator, or device results yet.

`PageIndex` combines the planner, simple stream and incremental paginator over
one immutable packaged EPUB snapshot and configured-font snapshots. It caps
the EPUB at 16 MiB, the font stack at eight files totaling 16 MiB, and the
index at 30,000 line records and 16,384 pages. It disables system font
discovery and embedded fonts, rejects missing glyphs or a shaped font outside
the captured stack, and derives source and computed styles from the same
snapshot and exact settings. It stores original-source line ranges, page
bounds, batch-to-line and page-to-line ranges, and continuation state at each
source-block batch boundary. Replay verifies a SHA-256 transcript of original
source ranges, line and paragraph geometry, selected font face, variation and
synthesis settings, clusters and positioned glyphs before the live Reader
paints the page. The index returns nothing if a batch fails or is cancelled.
Digests check cancellation between 64 KiB chunks; a single ZIP parse, font
registration or paragraph shape is not interruptible.
Scroll, facing-page, hidden-flow, nonlinear-hiding and in-page note modes
remain excluded.

The proof covers the in-memory EPUB/font bytes and supplied settings,
user-style and hyphenation snapshots. The session now binds those bytes to its
open book with before/after file stamps and a live chapter comparison, but it
does not prove system-fallback font equivalence. It also does not yet establish
live Reader correctness for Find, selections, navigation or saved-position
handoff. Peak allocator and device memory remain unmeasured. Transcript checks
prove replay against the captured index; they do not replace comparisons with
ordinary Reader layout or KOReader. No behavioral, simulator, or device tests
have run.

The session uses a font capture helper to retain configured font files in
order as immutable byte snapshots. It allows at most eight regular files and
16 MiB in total, refuses a symlink as the final path component, hashes each
file while reading, and checks cancellation between 32 KiB reads. Descriptor
stamps are checked before and after each file and every named file is checked
again after capture. This does not snapshot system-discovered fallback fonts
or prove those fonts equivalent. Revalidation compares metadata and does not
rehash file contents; parent path components are not traversed with no-follow
descriptors. The storage lease spans the bounded capture; latency and peak
memory remain unmeasured.

## Whole-book page navigation

The asynchronous screen-page index can include the same supported windowed
chapter route when the EPUB and configured font stack fit both workers' bounds.
The background pass captures the EPUB and ordered font files once, then builds
the long-chapter `PageIndex` from those immutable bytes. It compares the
chapter's replayed page count with the foreground witness before publishing the
whole-book page totals, headings, or custom targets. The captured path and
font stamps are checked again before accepting the completed index. This lets
Book Map and document-page navigation include an eligible chapter up to 1 MiB
instead of silently losing whole-book page numbering for that chapter.

This remains a narrow candidate: the chapter must still pass the windowed
renderer rules, and the book must fit the existing 8 MiB archive, spine, and
font-index limits. A failed, cancelled, changed, styled, or oversized source
does not publish partial totals. `chapter_offsets` remains capped for reading
progress weighting; this page index uses the exact chapter member size for its
own bound. No behavioral, simulator, or Kindle acceptance has run.
