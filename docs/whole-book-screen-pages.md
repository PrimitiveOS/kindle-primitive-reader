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
track and Exit. The optional header uses the exact page fraction for its
percentage, including when it shows publisher or generated edition labels.
It keeps the existing progress estimate until exact screen pagination is ready.
The bottom progress track and Contents marks have their own position rules.
[Installed layout observation](../src/session.rs) · [Validated counts](../src/session/screen_pages.rs) · [Background worker](../src/session/screen_pages/background.rs)

## Cache and storage

The count is held in memory for one generation. Its identity covers the
ordered spine, publication revision, current viewport and header reservation,
typography, publisher styles and user CSS, hyphenation, and the ordered
reading and fallback font file stamps. Raster contrast, gamma, overlap clues,
night-image inversion, and the linked-margin switch reuse the current page
counts when the page geometry and layout are unchanged. KOReader's gamma
handler also repaints the current view, while its font-size and CJK-width
changes request a position update and repagination. [Pinned KOReader font controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfont.lua#L203-L275)

The Probe's EPUB publication revision
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

The Mac simulator framed the same authored chapter before and after counting
48 screen pages. The page text, chapter footer, progress track and Exit
matched below the header. [Counting pages](screenshots/whole-book-screen-pages-pending.png) · [Whole-book total](screenshots/whole-book-screen-pages-complete.png)

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

## Oasis 3 test results

The 0.1.35 ARMHF candidate passed 1,069 Reader library tests, 14 EPUB package
tests and four layout checks. It skipped 63 optional tests. The native Harbor
Notebook frames matched below the 48-row header before and after counting 48
pages. The Kindle and Mac link and dictionary-settings frames matched. The
expanded dictionary frame differed in three grayscale pixels.

In an original 30-section EPUB from Documents, Reader painted the first frame
in 1,555 ms and completed its count in 7,889 ms from opening. The background
worker and independent foreground layouts agreed on 430 pages. Peak resident
memory during the background check was 39,880 KiB. The selected reading
position and all pixels below the header matched. The original EPUB's
SHA-256 matched before and after the closed-profile check.

These checks use software-rendered Kindle frames and an isolated profile.
Physical touch, refresh and USB sequences have separate acceptance checks.

## Native verification recipe

Build the ARMHF test bundle from the candidate commit with
`python3 scripts/prepare-device-tests.py --offline build/device-pagination`.
Use a dedicated directory on user storage and run the tests from there.
Keep the existing Launcher and Reader install in place. Record the exact
tested commit, executable checksum, firmware and device profile.

Run the authored Harbor Notebook case first. It opens at page 1, renders the
fallback, waits for the background worker without turning the page, and
captures the completed `1/48` header. Compare frame bytes below the first
48 rows and confirm the original anchor, chapter footer, division marks and
Exit. The repaint test keeps the number stable through gamma, contrast and
other paint changes, then drops the old total after a font-size change.
The cancellation test drives visibility, reflow, book switching and USB
return in the app model.

```sh
umask 077
run=$(mktemp -d /mnt/us/reader-pagination.XXXXXX)
export TMPDIR="$run" READER_TEST_FIXTURES="$PWD/fixtures"
READER_BACKGROUND_SCREEN_PAGE_BEFORE="$run/before.png" \
READER_BACKGROUND_SCREEN_PAGE_AFTER="$run/after.png" \
  ./reader --exact app::tests::screen_pages_background_finishes_the_latin_book_without_a_page_turn
./reader --exact session::screen_pages::tests::screen_pages_keep_the_completed_count_through_repaints
./reader --exact session::screen_pages::background::tests::screen_pages_background_discards_stale_work_on_visibility_reflow_and_usb
```

Run the original EPUB check against the authorized file in Documents. Pick a
regular DRM-free EPUB with 2–32 spine items within the snapshot limits. Use
the Reader font stack, including existing firmware fallbacks where present,
as a colon-separated `READER_TEST_FONTS` value. The original path is opened
directly. The test saves only a scratch profile, confirms the original
bytes, sums foreground chapter counts for an independent reference and emits
only aggregate page, timing and peak-memory values. Allow up to 90 seconds
for the exact total and 240 seconds for the entire run, including reference
layouts. Keep private filenames, file hashes and displayed personal pages
out of the test report.

```sh
READER_PARITY_EPUB="$APPROVED_EPUB" READER_TEST_FONTS="$READING_FONT_PATHS" \
  timeout 240 ./reader --ignored --exact \
    app::tests::screen_pages_background_verifies_an_original_epub --nocapture
```

Collect the two authored PNGs from the device before removing the scratch
directory. These are ARM-rendered app frames. A foreground framebuffer
capture is a separate check. With the authored EPUB displayed through
Launcher, use the existing SSH transport to stream `/dev/fb0` to the Mac.
Confirm Y8, a 1,280-byte stride, 1,264 × 1,680 visible dimensions and zero
x-offset. Crop each visible row to 1,264 bytes and start at the current
y-offset. Convert the cropped rows to an 8-bit grayscale PNG. On the Oasis,
this avoids a 4.5 MB staging file on its small `/var` volume. For the physical
handoff, record one foreground timing sequence with the header disabled and
enabled, turn across a chapter boundary during counting and try one
sleep/wake and one USB eject. Use the snapshot limits and separate renderer
totals when comparing against the pinned KOReader reference.
