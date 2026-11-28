# KOReader content zoom and fit choices

KOReader separates the zoom target from the way it fills the screen. In the
pinned fixed-page controls, **Zoom to** selects the page or detected content;
the **Fit** choice then uses the full viewport, its width, or its height.
KOReader obtains the content box through its used-bounding-box path and applies
the selected fit axis in the zooming module.
[Pinned controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L158-L339) ·
[Zoom dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerzooming.lua#L344-L470)

KOReader v2026.07.1 defaults new fixed-page sessions to **Page · Width**
(`pagewidth`). Reader's document defaults now use the same initial mode;
content zoom remains an explicit user choice.

## Reader candidate

Reader's fixed-page View controls expose **Zoom to: Page/Content/Columns/Rows/Manual**
and **Fit: Full/Width/Height**. The fit axis is retained across all five zoom
targets, including tile and Manual modes. Switching between Page and Content
keeps the selected fit axis; choosing Page exits a tile target and restores the
axis. Column and row counts are stored independently. Manual is a separate
KOReader zoom genus: it trims to detected content, fits width, and uses the
manual zoom factor. Content fitting renders the page to a bounded grayscale
raster, normalizes contrast against the raster border median, detects ink
bounds, discards isolated specks, adds the saved crop margin, and scales the
result to the selected viewport dimension. Pixels near the sampled background
become white and contrasting content becomes dark, so white text on a dark
page and dark ink on a light page use the same bounds path. This modifies only
the detector's scratch raster, not the document or displayed page. A mixed
border keeps the legacy dark-ink path to avoid treating a full-bleed image as
the page background. The detected rectangle is normalized to the original page
and used by the existing fixed-page transform.

The candidate runs the same detector for the current fixed-page providers:
PDF, DjVu, fixed-layout EPUB, comic archives, GIF, TIFF, and standalone images.
It caches detected boxes by page and margin and saves the fit mode with book
state, fixed-document defaults, and reading profiles that contain fit and zoom.
The raster is capped at 1536 pixels on its longest side. This is a native
approximation of KOReader's content-box behavior; it does not call KOReader's
`k2pdfopt` provider. The implementation and provider behavior have not yet been
verified, so this is not a parity claim.

## Deferred acceptance

- [ ] Compare the control sequence, labels and selected states against the
  pinned KOReader panel, including changing fit axis before and after switching
  between Page and Content targets.
- [ ] Compare background normalization and check content detection on real PDFs,
  DjVu books, fixed-layout EPUBs, comics, GIF/TIFF files, and images, including
  white-on-dark pages, sparse
  notes, blank pages, low contrast, page-edge marks, and changing bounds across
  pages. Confirm the bounded raster stays responsive and memory-limited.
- [ ] Exercise saved crop margins at their minimum, default, and maximum, then
  page turns, rotation, manual crop, fit width/height, zoom, spreads, profiles,
  reopen, and saved viewport positions.
- [ ] Verify source-coordinate alignment for links, selectable text,
  highlights, annotations, and return-to-source navigation after a detected
  crop. Blank or undecidable pages must fall back without hiding content.
- [ ] After implementation work is complete, run unit, functional, and system
  checks, capture simulator screens, compare them with KOReader, and validate
  the verified build on Kindle.
