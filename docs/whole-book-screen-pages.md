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

## Implemented foundation

The optional top header reads an exact prefix once Reader has successfully
laid out and displayed every spine item under one regular paged EPUB layout.
Before then it shows its existing chapter-local numbers. Edition labels take
precedence. Each sample carries the chapter's source revision and leaf count.
The in-memory generation checks the ordered spine, active settings and
publication revision, applied user CSS and hyphenation, and the archive and
ordered fallback font file stamps. A file changing during load loses its
sample. USB return clears the generation. This slice accepts horizontal,
single-leaf Parley layouts from complete EPUB archives.
[Installed layout observation](../src/session.rs) · [Validated counts](../src/session/screen_pages.rs)

## Background completion

- Start when the paged reflow header is visible and requests a screen number
  or count. Use valid edition labels immediately when available. Schedule the
  count after the first painted page. A header toggle, book switch, or
  layout-affecting change reevaluates demand and cancels obsolete work.
- Reuse the active Probe's count and the same layout function for other
  spine items. Count leaves using the current engine, width and height,
  margins, font resolution, DPI, style rules, hyphenation and writing mode.
  Retain counts and checked prefix sums. Release each temporary layout before
  building the next. A current chapter at index `i` uses the sum of leaves
  before `i` plus its one-based page index. The last prefix is the total.
- Keep the chapter-local display until every required count is positive,
  present and tied to the same edition and layout generation. Reject a result
  for an unreadable or recovered-placeholder chapter, overflow, inconsistent
  spine order, stale delivery, cancellation, or any count that disagrees
  with the active Probe. Treat a deliberately blank rendered leaf as a
  counted page. Failures leave the document readable and its location intact.
  A permanent failure ends that generation. Retry on a new generation or
  when storage returns after a transient interruption.
- First support regular paged reflow. Keep the documented fallback for hidden
  flows, continuous scrolling and facing pages until flow membership and
  spread semantics have independent coverage. Keep the normal
  `Chapter x of y - Page m of n` footer, track and Exit. Apply whole-book
  screen percentages and Contents markers only after their own page
  definitions and anchor mapping have been verified.

## Cache and storage

Start with an in-memory entry keyed by the validated library edition ID,
ordered spine, renderer and effective layout options. Include viewport and
header reservation, reading mode, base and publisher style settings, applied
user stylesheet identity, hyphenation rules, and the ordered reading and
fallback font identities.
Use a generation token so a completed worker cannot update a newer layout.
In EPUB, the Probe's publication revision comes from the package XML and can
survive changes to XHTML, CSS, images or embedded fonts. Use the library's
validated publication identity as well. A durable cache also needs a
versioned layout format and resource identities, including fonts whose files
can be replaced at the same paths. [Publication revision](../src/epub.rs) · [Edition identification](../src/library.rs)

The Kindle storage helper holds a process-wide lease mutex for the callback.
The current chapter layout runs inside that callback. Moving that call as-is
to a worker could still delay foreground storage access. First separate a
bounded read of owned chapter resources from shaping and layout. Consume only
owned data after releasing the lease. Allow one job at a time and cancel or
suspend between chapters on sleep, USB export, or a changed book or layout.
Validate the edition after storage returns before accepting a result. Start
work in a worker from an owned snapshot because the current BookConfig holds
thread-local reference-counted data. [Pinned lease implementation](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-app/src/storage.rs#L26-L58) · [Existing cancellable search worker](../src/app/nearby_search.rs)

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
