# Whole-book screen page numbering

KOReader renders a single EPUB DOM containing the spine fragments before
reporting the document ready. Its screen page count comes from that rendered
page list. Its current page comes from the active position and the Lua bridge
adds one to the zero-based index. Facing pages ordinarily count as two leaves.
CREngine can also count a two-leaf spread as one page. [EPUB DOM construction](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L1874-L1891) · [Load and render](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/readerui.lua#L329-L345) · [Count after rendering](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/readerui.lua#L514-L517) · [Lua page bridge](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L897-L918) · [Count and spread conversion](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L2249-L2255)

Stock status controls prefer the current and last edition labels when enabled.
Otherwise they use flow-local ordinals and counts for hidden flows, or
whole-document screen numbers. The alternate header's percentage is based on
screen pagination or the active flow. [Alternate header](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readercoptlistener.lua#L90-L132) · [Footer page items](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L243-L292)

Reader lays out one spine item at a time. Adjacent cached bitmaps serve page
turns within that layout. The optional header already resolves valid publisher
or generated edition labels. Its screen-number fallback uses the current
chapter's page index and count. Whole-book progress and time for reflowable
books with several spine items use source-byte weights. A source-byte offset or
character-count edition page cannot establish the count of rendered leaves.
[Chapter layout](../src/session.rs) · [Edition labels](../src/app/publisher_pages.rs) · [Reading status calculations](../src/app/reading_footer.rs)

## Exact page count

The optional header shows its chapter-local numbers while Reader gathers
screen page counts. Valid publisher or generated edition labels take
precedence. The header starts a background count when the reading screen is
visible and a screen number or count is enabled. The first page is painted
before the worker starts. Visiting a chapter also records its count and
source revision through the normal page layout.

A result needs a positive count for every spine item in one layout. The
worker uses the same publisher resources and Parley pagination as normal
reading. Its first count must agree with the visible chapter. A missing or
recovered-placeholder member, unsupported writing mode, changed identity,
invalid prefix or conflicting sample leaves the header chapter-local. The
default reading footer uses `Chapter x of y - Page m of n` with its progress
track and Exit. Whole-book percentages and Contents marks have separate
position rules.
[Installed layout observation](../src/session.rs) · [Validated counts](../src/session/screen_pages.rs) · [Background worker](../src/session/screen_pages/background.rs)

## Cache and storage

The count is held in memory for one generation. Its identity covers the
ordered spine, publication revision, current viewport and header reservation,
typography, publisher styles and user CSS, hyphenation, and the ordered
reading and fallback font file stamps. The Probe's EPUB publication revision
comes from the package XML. The worker also checks archive and font file
stamps before and after taking its snapshot, and again before delivering the
counts. USB return clears the generation. A future durable cache would need
the validated library edition ID, a versioned layout format, and stable
resource identities.
[Publication revision](../src/epub.rs) · [Edition identification](../src/library.rs)

The worker snapshots a complete compressed archive and the non-firmware
reading fonts in 128 KiB reads. The Kindle storage lease ends after each
read and the file handle closes with it. EPUB decompression, source parsing,
CSS, image decoding and text layout use the snapshot after the lease is
released. Firmware fonts use the usual mapping on the always-mounted
firmware volume. The worker releases each temporary layout before the next
chapter and delivery passes the active generation check on the main thread.
Leaving the reading screen, sleep, USB return, or changing the book or
layout cancels or discards the scheduled work.
[Pinned lease implementation](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-app/src/storage.rs#L26-L58)

Background snapshots cover regular horizontal paged EPUB archives up to
8 MiB compressed and 64 MiB expanded, with at most 192 spine items and
256 KiB per chapter. The reading stack can include up to 64 font files,
with a combined 16 MiB budget for non-firmware files. Books above these
limits can still collect exact counts through visited layouts where
supported. Continuous scroll, facing pages and hidden flows retain their
established page-number semantics.

## Acceptance checks

Use the authored EPUB 2 and EPUB 3 books for boundary navigation and
`many-chapters.epub` for pending work, cancellation and bounded memory.
Add cases with a changed font, page size or header height, duplicate labels,
an unreadable spine item, and an edition changed while indexing. For each
completed generation, compare its total and prefix ordinals against the
actual frames visited from first to last. Cross a spine boundary and reopen a
saved passage. Compare the same source anchors in stock and Reader and check
each renderer's own page sequence. Their different layouts can produce
different numeric totals.

Capture fallback and completed header frames in the simulator. Test USB
export, eject, sleep and wake with the first page already visible while the
count runs. Record first-frame latency, page-turn latency and peak RSS on the
Oasis before and after. Check that the default footer pixels, Exit, reading
position, and saved annotations stay intact.
