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
and canonical text, 8,192 blocks/elements, 128 KiB per batch or block and 32
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
