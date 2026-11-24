# Reading orientation and Launcher handoff

KOReader’s bottom Rotation control changes the whole reading screen. It offers four modes for reflow and fixed pages. A turn within the same aspect ratio requests one full repaint. A portrait-to-landscape transition also changes screen dimensions and lays out the document again. With rotation unlocked, the saved book choice takes precedence over the global preference. The stock framebuffer code composes its logical turn with the native framebuffer and blitter turns. [Reflow rotation choices](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L49-L101) · [Fixed-page rotation choices](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L39-L94) · [Live resize](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L892-L927) · [Load](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L958-L975) · [Save](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L1209-L1222) · [Framebuffer turn](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/ffi/framebuffer.lua#L452-L462)

Reader at `a333686` saves an application-wide preference with Follow Launcher, Upright and Upside down. Fixed-page quarter turns change the page within that screen. The shared app kit at `50035cf` makes a portrait canvas at startup, accepts a portrait override through `preferred_rotation`, and redraws after a turn without a viewport callback. Software drawing accepts firmware framebuffer turns 0 and 2 and compensates for them independently of the app’s requested turn. The display and snapshot code use native portrait dimensions. [Reader controls](../src/app/bottom_config.rs#L77-L98) · [Fixed-page control](../src/app/bottom_config.rs#L898-L909) · [Reader runtime hook](../src/app.rs#L8133-L8141) · [App kit contract](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-app/src/lib.rs#L59-L82) · [Canvas initialization](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-app/src/lib.rs#L400-L433) · [Panel constraints](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-eink/src/lib.rs#L221-L242)

## Reproduced portrait input mismatch

The running app and Launcher have separate input transforms. The app kit publishes its turn to its process-local input mapper and the RAM `orientation` file. In Launcher 0.6.22, the foreground-app branch checks the gesture already interpreted in Launcher’s turn. Its overlay snapshot is also interpreted using Launcher’s settings. This leaves the system gesture without a single coordinate owner when an app overrides the turn. [App publication](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-app/src/lib.rs#L369-L373) · [Rotation update](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-app/src/lib.rs#L548-L560) · [Launcher dispatch and backdrop, `155ce3f`](https://github.com/PrimitiveOS/kindle-primitive-launcher/blob/155ce3fd6e520d9c4fc69e1a39d9c157d5135fc8/crates/kl-launcher/src/main.rs#L657-L678)

A Mac harness compiled the production point transform, gesture recognizer, both system-swipe predicates, page-key map and frame transforms. Vectors use a 1264 × 1680 panel, x = 632 throughout and a 120 px Launcher status bar. Both aligned control cases, with Reader and Launcher at turn 0 or both at turn 2, recognized the visible top-edge swipe in both predicates. The split case produced these results. [Touch and keys](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-input/src/lib.rs#L140-L159) · [App predicate](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-app/src/lib.rs#L334-L338) · [Launcher predicate and backdrop transform](https://github.com/PrimitiveOS/kindle-primitive-launcher/blob/155ce3fd6e520d9c4fc69e1a39d9c157d5135fc8/crates/kl-launcher/src/system_layer.rs#L145-L158)

| Native y coordinates with Reader at 2 and Launcher at 0 | Reader input result | Launcher input result |
| --- | --- | --- |
| 1660 → 1400, down from Reader’s visible top | Starts at 19 with dy +260. Drops a system gesture. | Starts at 1660 with dy −260. Leaves Quick Settings closed. |
| 20 → 280, down from Launcher’s portrait top | Starts at 1659 with dy −260. Passes the swipe to Reader’s normal input path. | Opens Quick Settings. |

The same harness fed a four-pixel marker through Reader’s output and Launcher’s backdrop conversion at framebuffer turns 0 and 2. Using the app turn recovered `[1, 2, 3, 4]` in both cases. Using Launcher’s turn recovered `[4, 3, 2, 1]`. This establishes the coordinate mismatch in the backdrop buffer. It leaves on-device overlay timing, snapshot restoration and actual firmware behavior for acceptance testing.

KOReader maps the Oasis 3 raw key codes `104` and `109` to forward and back at turn 0. Its rotation map swaps them at turns 1 and 2. PrimitiveOS maps those codes to back and forward at turn 0 and swaps them at turn 2, as confirmed by the Mac harness. Preserve the established PrimitiveOS mappings for portrait while designing and testing the two landscape directions. Keep Reader’s page-button inversion as an independent preference. [KOReader Oasis keys](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/device/kindle/device.lua#L1641-L1665) · [KOReader rotation map](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/device/input.lua#L275-L282) · [PrimitiveOS Oasis profile](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/50035cf068c0157abf97477ef64eb28bc984b6b7/crates/primitive-device/src/profile.rs#L28-L41)

## Implementation and acceptance order

1. **Align the existing portrait path first.** Give the foreground session an authoritative effective turn, tied to the current app and frame generation. Both processes must use it for the reserved edge gesture. Associate the saved pixels with their app turn, framebuffer turn and dimensions. Clear foreground orientation on exit or crash. When a snapshot no longer matches after an overlay, ask the app for a redraw through the existing handoff. Keep Launcher’s own portrait preference independent.
2. **Add a shared viewport transaction.** Extend the app contract with four logical turns and a canvas resize acknowledgement, retaining a portrait default for apps that use the existing contract. Apply one consistent configuration to the canvas, draw transform, touch endpoints and physical keys. For Oasis, first validate software quarter turns over the supported portrait framebuffer modes. Cancel or discard gestures and queued input from an incompatible viewport generation. A rejected change leaves the last valid viewport usable.
3. **Integrate the Reader control.** Build layouts and hit areas from the active viewport. Restore EPUB through its source anchor and fixed pages through their source page and visible point. Apply and persist the book’s turn only after the viewport succeeds. Existing settings supply the fallback for books without a saved turn. Keep fixed-page content rotation independent.

| Acceptance | Check |
| --- | --- |
| App-only portrait | Force each portrait orientation while Launcher stays opposite. Apply live from the bottom bar and verify that text, selection, bookmarks and the current EPUB source anchor survive. Close and reopen two books with different choices. Verify the legacy fallback and the prior source position. |
| Landscape resize | Exercise both landscape directions with single and facing reflow pages, continuous reflow and a zoomed PDF. The logical viewport becomes 1680 × 1264. Check stable source anchors across width changes, correct page and scroll direction in left-to-right and right-to-left books, reachable reading controls and one full frame on each successful turn. Compare an authored fixture with an authorized original in a scratch profile. |
| Coordinates and physical keys | Use labeled corners, tap targets and a held selection in all four views at firmware turns 0 and 2. Round-trip both swipe endpoints and verify the physical page-key pair, the separate inversion setting and a turn during an in-flight gesture. Verify rotated PDF links and crop targets with independent document rotation. |
| Launcher overlays | From the visible top edge, exactly one Quick Settings action opens. Repeat with sleep and wake, USB connect and eject, theme changes, another turn while an overlay is open, and app exit or crash. Check alignment and valid restore or redraw, then confirm Launcher retains its own portrait preference. Run the portrait mismatch vectors in the Mac suite and the foreground paths on the Oasis. |

## Book Map and Page Overview

KOReader computes both visual navigation surfaces from the current screen dimensions. Book Map centers on a source page and lets the reader browse the map before opening a destination. The page browser computes its grid and ribbon from the available width and height. Selecting a thumbnail records a return location, and holding a thumbnail provides bookmark access. Physical page buttons and gestures browse the grid. [Book Map sizing and focus](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/bookmapwidget.lua#L697-L715) · [Book Map input](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/bookmapwidget.lua#L1776-L1825) · [Page browser sizing](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/pagebrowserwidget.lua#L320-L369) · [Page browser actions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/pagebrowserwidget.lua#L1586-L1703)

PrimitiveOS Page Overview adapts its four thumbnail slots from two rows in portrait to one row in landscape. Vertical swipes move by a row. Horizontal swipes, buttons and physical keys move a batch. The track browses chapters in EPUB and source pages in fixed books. Resizing keeps the previewed EPUB source anchor visible even while browsing a different chapter. Book Map repaginates the reflow chapter list around a visible chapter and projects its fixed-page groups from six columns by eight rows to eight columns by six rows. Selecting an entry preserves the prior reading location in jump history.

The host `visual_navigation` tests render authored EPUB and PDF fixtures through the viewport transaction at 1264 × 1680 and 1680 × 1264. They cover both landscape turns, portrait return, aborted presentation, far-right taps, track swipes, physical page events and bookmark actions. Set `READER_VISUAL_NAVIGATION_CAPTURE_DIR` when running `cargo test --locked --lib visual_navigation` to write the portrait and landscape captures.

[Page Overview](screenshots/landscape-page-overview.png) · [Fixed-page Book Map](screenshots/landscape-fixed-book-map.png) · [Reflow chapter map](screenshots/landscape-chapter-map.png)


## Unlocking a PDF

The secure password prompt keeps the accepted turn and masks typed characters.
A failed password leaves the same prompt open. Cancel restores the previous
book or browser, and authentication prepares the saved PDF page at the
accepted size before applying the edition’s orientation preference.
[Masked landscape prompt](screenshots/landscape-unlock-pdf.png)


## Cropping a fixed page

The crop preview holds the accepted view until Apply or Cancel. The handles,
crop press targets, and footer follow the active dimensions. Reader maps the
touch point through the independently rotated PDF to the source page. Cancel
restores the accepted reading frame and Apply saves the source rectangle.
The authored PDF test verifies both paths and the original file digest.
[Landscape crop preview](screenshots/landscape-pdf-crop.png)


## Foreground acceptance on Oasis 3

The Reader development entry advertises viewport handoff protocol 2 in its
package header. The foreground test uses a Launcher compiled with
`viewport-handoff` and a supervisor started with
`KL_EXPERIMENTAL_VIEWPORT_HANDOFF=1`. Both binaries use Tools revision
`c8ee49b764c96c013d0fa8e9d79dfdeddbdaaecc`. Set the opt-in on the
scratch supervisor.

Start from Amazon’s Library after the installed supervisor has exited.
Use a scratch Launcher payload, a scratch Documents folder, and the
supervisor’s runtime entry with `KL_DIR` and `KL_DOCUMENTS` pointing to
those folders. Give the isolated session a separate `KL_RUN` under
`/var/tmp/kl-reader-*` and pass the same value to the capture helper. Record the manual `Documents/Launcher.sh` checksum before
starting the scratch session. The launcher tile starts the scratch Reader through the normal session
creation path. Its reader home and document directory contain authored
fixtures and private test state.

| Checkpoint | Evidence |
| --- | --- |
| Foreground presentation | Run `scripts/verify-oasis-foreground.sh preflight` at the scratch Launcher grid. Open Reader from the grid and validate the ready record against the live PID, process start tick, scope, stride, framebuffer turn and row offset. Capture the authored EPUB in portrait and both landscape directions. |
| Input and overlay | Check both physical page keys and a body swipe. Open Quick Settings from the top edge. Observe the stopped Reader process and held page-key fence. Compare the awake frames before and after closing an unchanged overlay. A queued page key pressed under the overlay leaves the page intact. A fresh page key advances it. |
| Sleep and USB | After the power-button sleep cycle and after a data-cable mount and host eject, check the resumed source page, fresh ready generation and page-button input. |
| Rollback and exit | Reject the oversized vertical EPUB draft viewport and check that the prior page remains available. Exit Reader to the portrait Launcher, then to Amazon’s Library. Verify removal of the scratch session records and compare the installed startup files with their baseline checksums. |

The capture helper checks the live fixture process around each awake
framebuffer read. Use `capture-overlay` with the same fixture home to read the Quick Settings
frame while the app is stopped. It requires the live scoped offer, the ready
frame and matching geometry, the occlusion marker, and a held page-key fence.
Visible panel refresh, the physical controls, a power cycle, and a USB data
session require complementary hardware evidence.


### Staged ARMHF build

Reader `1e8438d` and Launcher `65e47b5` produced static ARM hard-float
executables using the locked build. The scratch session passed the staged
executable, supervisor, launch header and fixture checksums. Reader’s
package carries both project licenses, the OCR executable and model, and
the protocol marker in the first ten lines of the installed entry. The
Mac passed 1,277 library tests, 14 EPUB package tests, and four layout
probes against this Reader source. The Oasis ran the same source in
ARM hard-float and passed 1,281 library tests, 14 EPUB package tests and
four layout probes. The library suite adds four device-clock checks on
Linux. The native runner used an isolated POSIX temporary directory for
fixtures with Unix-only filenames.
[Launcher source and handoff notes](https://github.com/PrimitiveOS/kindle-primitive-launcher/blob/65e47b5da8338924932a49f2bb7953efce8f24a7/docs/launcher-viewport-handoff.md)


### First live Oasis session

The isolated Launcher read the Reader tile from its scratch Documents directory.
An injected touchscreen tap opened Reader through the normal Launcher session.
Reader presented the authored EPUB in portrait and accepted both landscape turns
in the same process. Each landscape ready record showed 1680 × 1264 logical
pixels over a 1264 × 1680 native panel, stride 1280, firmware rotation 0 and pan
0,0. A swipe inward from the bottom of the right-hand landscape view opened the
Reader controls. Injected events on the Kindle’s gpio-keys evdev node moved
between pages 2 and 3 in both directions. Page 3 had the same projected frame in
both landscape turns. The far-right Exit button returned to
the portrait Launcher with exit code 0. Reopening Reader displayed its saved
Continue reading entry, and the authored Japanese EPUB rendered in its vertical
writing mode. The authored two-page PDF advanced with an injected page key
and returned to its original framebuffer bytes with the other key.

[Portrait EPUB](screenshots/oasis-viewport-epub-portrait.png) ·
[Landscape EPUB frame shared by both turns](screenshots/oasis-viewport-epub-right.png) ·
[Vertical Japanese EPUB](screenshots/oasis-viewport-vertical-epub.png) ·
[Portrait PDF](screenshots/oasis-viewport-pdf-portrait.png)

The device screenshots are framebuffer readbacks. The input exercised the
Kindle’s evdev path by injection. Mechanical key timing, panel refresh appearance,
sleep and USB data resume still require their foreground acceptance checks.

The first landscape frame exposed a stale Screen selection until the next
redraw. Reader now renders the matching pending orientation in the staged frame
and saves the edition preference on commit. It also sizes toolbar tap zones and
custom swipe edges from the accepted viewport. The host regressions exercise
both landscape turns and preserve the existing portrait input boundaries.


### Live retest of the landscape fixes

Reader `f3215a3` used the same scratch Launcher and reopened the first
session's saved EPUB in Landscape Left. A lower-center tap brought up the
Screen controls. Tapping Right replaced the Left underline with the Right
underline in the first ready framebuffer. An upper-center tap in Landscape
Right, outside the corresponding portrait tap zone, opened the reading
toolbar. Both ready records matched the live app identity and its isolated
viewport path. Exit returned to the portrait Launcher. The scratch
session handed back to Amazon’s Library before reopening the installed
Launcher. The manual Library entry, Launcher binary and installed
supervisor matched their baseline hashes.

The same ARM binary opened, advanced and resumed four original documents
in separate scratch profiles, one each in EPUB, PDF, AZW3 and MOBI. Each
original matched its starting SHA-256 digest.

[First live Landscape Right frame](screenshots/oasis-viewport-first-right-frame.png)


### Live Quick Settings handoff

The scratch Launcher `65e47b5` and Reader `f3215a3` exercised Quick Settings
on the Oasis in both landscape directions. A top-edge swipe stopped the
same Reader process and set the scoped page-key fence to held. A handle tap
arrived in Launcher at logical (840, 942) in both directions. The 286 rows
below the overlay remained byte-identical to the reading frame.

An injected Page Forward reached Launcher while Quick Settings was open.
An upward swipe dismissed the overlay, resumed Reader, and restored all
2,123,520 visible framebuffer bytes without a page turn. A fresh Page
Forward in Landscape Right advanced once from page 2 to 3, then Page Back
restored the same page 2 framebuffer. The capture helper rejected a
normal read while Reader was stopped and an overlay read while it ran.

The inputs used the device's touchscreen and gpio-keys evdev nodes. The
checks cover routing, ownership, and restore at firmware framebuffer
rotation 0. Digitizer response, physical key timing, panel waveform
appearance and the sleep and USB cycles remain separate.

[Quick Settings over Reader on Oasis](screenshots/oasis-viewport-quick-settings.png)
