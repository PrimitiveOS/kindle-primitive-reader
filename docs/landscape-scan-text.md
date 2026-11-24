# Scan text in landscape

**Scan → Read text** and **Scan → Reflow scan** use the same orientation as
the PDF or DjVu. Reader lays out the source page again at the new size
and shows the screen containing the passage that was already visible.
Read text keeps its text anchor. Reflow scan keeps the recognized word’s
ordinal on the original source page. Returning to portrait before turning
the page restores the original frame.

The footer, spacing panel, scan size buttons, Back, and page-turn areas
follow the live viewport. Reader prepares both the fixed document view and
the scan text view before accepting a new size. Rejecting the frame, changing
the open panel during preparation, or losing the source OCR leaves the
previous view available.

## KOReader comparison

Pinned KOReader recalculates the fixed view and initializes its scroll pages
when a rotation changes the screen dimensions. Its k2pdfopt context receives
the current canvas width and height at the device DPI, and its position
transforms map reflowed locations to and from the native page. Reader keeps
the same physical text scale and resolves its own typed source anchor through
each new layout. [Rotation and viewport](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L892-L927) · [Context dimensions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L133-L157) · [Position transforms](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L1349-L1364)

## Mac verification

Run `cargo test --locked --lib scan_text_viewport`. The tests use page two
of the authored Harbor survey in vector-text and raster-only PDF form. The
raster version uses saved OCR word boxes produced from its original page
image. The tests exercise the exact source anchor, 1264 × 1680 and 1680 × 1264
frames, staged abort and retry, far-right touch and hardware page-button input,
spacing cancellation, scan size changes, and Back. Set
`READER_SELECTION_VIEWPORT_CAPTURES` to save the simulator frames. [Read text](screenshots/landscape-read-text.png) · [Read text spacing](screenshots/landscape-read-text-spacing.png) · [Reflow scan](screenshots/landscape-reflow-scan.png) · [Reflow scan spacing](screenshots/landscape-reflow-scan-spacing.png) · [Fixture generator](../scripts/generate-scan-reflow-fixtures.py)
