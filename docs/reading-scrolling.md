# Continuous scrolling controls

Switch the document to **Scroll**, then open **Settings › Reading gestures ›
Scrolling**. These preferences apply across continuous reflow and PDF
documents. The settings remain visible in paged view with a message that
they are available in continuous view.

- **Classic scrolling** moves the document by the distance the finger has
  moved since the previous sample.
- **Turbo scrolling** moves by the distance from the starting point at each
  sample.
- **On-release scrolling** moves the document once when the finger lifts.

The activation delay accepts 0 to 2,000 ms. The default is 500 ms when
Reader's bars can open by swipe, and 0 ms with tap-only bars. Tap the
adjustment buttons for a 100 ms step or hold for a 500 ms step. **Default**
restores the current default in the draft. **Set** saves the method's shared
delay and Back discards the draft.

The delay begins at the first movement sample. A quick vertical swipe
released before activation leaves the page where it was. An on-release
drag also needs a movement sample at or after the deadline. Waiting in
place and lifting leaves the page where it was. When a matching swipe
opens Reader's top or bottom bar, Reader restores the position from before
the contact. Touch holds still select text. The Launcher owns the system
quick-settings swipe.

## Reference and Mac verification

The replay runs the menu callbacks and the reflow and fixed-page pan
handlers from pinned KOReader. It checks visibility, defaults, method
selection, delay bounds, both direct and held step sizes, and the exact
movement traces for the three methods. It also checks a quick vertical
swipe, a stationary release after the deadline, and restoration when a
swipe handler takes over.
[Executable comparison](../scripts/compare-reading-scrolling.py) ·
[Stock menu and delay](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerscrolling.lua#L77-L224) ·
[Stock reflow](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerrolling.lua#L541-L688) ·
[Stock fixed pages](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L363-L538)

The Mac tests drive Reader with the same movement events used by the
foreground app. They check source location and saved state in EPUB,
late release through the sleep lifecycle, width-fit PDF offsets, gesture
rollback, and portrait and landscape views. The shared input tests cover
the opt-in contact classifier, stationary selection, all four coordinate
turns, stale viewport generations, and sleep and USB gates. Reader samples
live drags at a 200 ms interval for E Ink. KOReader also offers inertia on
its emulator and non-E-Ink platforms. This Reader workflow is for Kindle
and uses the stock E-Ink modes. Physical touch timing, redraw behavior and
gestures still need a foreground Kindle check.

Run `cargo test --locked --offline --features kindle --lib scrolling`.
Set `READER_SELECTION_VIEWPORT_CAPTURES` to an existing folder to reproduce
the screenshots. [Light settings](screenshots/scrolling-controls.png) ·
[Dark settings](screenshots/scrolling-controls-dark.png) ·
[Activation delay](screenshots/scrolling-delay.png) ·
[Continuous EPUB](screenshots/classic-epub-scrolling.png) ·
[Continuous PDF](screenshots/on-release-pdf-scrolling.png)
