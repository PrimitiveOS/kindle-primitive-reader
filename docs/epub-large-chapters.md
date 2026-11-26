# Markup-heavy EPUB chapters

Reflow EPUB spine XHTML and HTML can now use up to 1 MiB of encoded member
bytes and 1 MiB of decoded or recovered XML. This helps with markup-heavy
chapters whose reading text still fits the existing 512 KiB text limit. The
XML parser's node limit and the existing image, font and layout budgets still
apply. Package and container metadata, navigation documents and fixed-layout
EPUB XML retain their 512 KiB limit; static SVG has its separate smaller limit.
Text-heavy chapters above the layout text limit still fail clearly and are not
split automatically.

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

The larger memory bound is an implementation candidate, not a device acceptance
result. Decoder expansion, peak memory, real EPUBs, simulator behavior and
Kindle performance remain unverified.

## Internal planning groundwork

An internal `LongChapterPlan` can inspect an original EPUB chapter and propose
bounded batches of its existing canonical text blocks. It keeps the original
book/chapter identity, source revision, block keys and UTF-8 offsets, and binds
the plan to the same parsed source and original chapter bytes. Batches are
work ranges only: they are not pages, new spine items or persisted positions.
The planner does not participate in reading, seeking, search, or annotation
handling, and does not increase the current chapter limit.

The initial candidate is deliberately narrow: at most 1 MiB of original chapter
and canonical text, 8,192 blocks/elements, 128 KiB or 256 blocks per batch, 128 KiB per block and 32
batches. It rejects nonunique or unsupported chapters, stylesheet or user-style
inputs, images, tables, lists, ruby, raw whitespace, generated content, special
breaks, complex boxes, non-horizontal and unsupported reading modes. A
cancellation flag is checked before and after the bounded chapter read and
between blocks; an individual parser/read is not interruptible. The original
book and saved state are untouched. Anchor lookup currently rehashes up to
1 MiB of canonical text to detect source mutation; a future interactive path
must avoid doing that work on each repaint. This is groundwork only: computed-style
and layout-source binding, exact page-continuation state, resource/memory
accounting, source-verified asynchronous indexing and all reading consumers
must be integrated before it can enable oversized chapters. It has not been
behaviorally tested or used on a device.

The current identity check binds original XHTML bytes and canonical block keys
and text. It does not detect in-place changes to element, link, flow or style
metadata, and does not bind fonts or reading settings. It is therefore not
enough to authorize a layout handoff; that later stage needs a full
source/metadata, computed-style, font and settings proof.

An isolated incremental paginator can consume a complete, source-ordered
stream of already-positioned simple lines. It carries the last overlapping
interval and page origin across work batches and uses the ordinary paginator's
rules for layouts without publisher breaks, split spaces or fitted tails.
It checks the borrowed source, original block keys and UTF-8 boundaries, limits
the stream to 30,000 lines and 16,384 pages, and returns no partial bounds on
cancellation or failure. It also applies the existing Parley unpainted-tail
trim when given the final layout's exact painted bottom. A batch boundary
never creates a page boundary. This
component is not connected to reading: it cannot prove that a layout stream is
complete, that its absolute geometry is continuous, or that fonts and styles
match. The continuation layout and those proofs remain required before use.

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

This stream is not wired into `Probe` or Reader. It does not prove the font,
settings or complete metadata snapshot, establish a total device-memory cap,
provide a page/window cache, or implement search, selection, navigation or
resume across batches. Those are prerequisites before any user-facing path can
use it. The stream has not been behaviorally tested or used on a device.

An unwired `PageIndex` experiment combines the planner, simple stream and
incremental paginator over one immutable packaged EPUB snapshot and an ordered
set of immutable font byte snapshots. It caps the EPUB at 16 MiB, the font
stack at eight files totaling 16 MiB, and the index at 30,000 line records and
16,384 pages. It disables system font discovery and embedded fonts, rejects
missing glyphs or a shaped font outside the captured stack, and derives the
source and computed styles from the same snapshot and exact settings. It stores
original-source line ranges, exact simple-page bounds and batch-to-line ranges;
these ranges are not resumable layout checkpoints. It returns nothing if a
batch fails or is cancelled. Digests check cancellation between 64 KiB
chunks; every bounded batch also retains a SHA-256 transcript of its original
source ranges, line and paragraph geometry, selected captured font face,
variation and synthesis settings, clusters and positioned glyphs. A future
window replay must reproduce that transcript before displaying the batch.
A single ZIP parse, font registration or paragraph shape is not interruptible.
Scroll, facing-page, hidden-flow, nonlinear-hiding and in-page note modes
remain excluded.

The proof covers these in-memory EPUB/font bytes and the supplied settings,
user-style and hyphenation snapshots. It does not prove that they are the live
open book or that this isolated font context is equivalent to Reader's normal
firmware-mapped or system-fallback font context. It also does not provide a
windowed renderer, interactive navigation, Find, selections or saved-position
handoff. Peak allocator and device memory remain unmeasured. No reader path
uses this index; the transcript proves replay only within its isolated input
snapshot and does not establish live Reader font equivalence. Source and font
handoff proofs and all those consumers remain prerequisites to user-visible
support. No behavioral or device tests have run.

An additional unwired font capture helper can retain the configured font files
in order as immutable byte snapshots. It allows at most eight regular files and
16 MiB in total, refuses a symlink as the final path component, hashes each
file while reading, and checks cancellation between 32 KiB reads. Descriptor
stamps are checked before and after each file and every named file is checked again after
capture. This does not snapshot system-discovered or other fallback fonts, or
prove which faces the live Reader selected. Revalidation compares metadata and
does not rehash file contents; parent path components are not traversed with
no-follow descriptors. This is a single-writer, advisory input proof, not a
security boundary or permission to hand an index to the Reader. The storage
lease spans the bounded capture, and its latency and peak memory are unmeasured.
