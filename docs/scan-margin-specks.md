# Isolated specks in PDF margins

**Scan → Semi-auto crop** filters isolated ink that spans less than two
source points horizontally and vertically, subject to raster resolution.
A connected stroke can qualify by extending along either axis. Crop
edges use original upright PDF points, including PDFs with a page
rotation. Reopening a saved crop restores its source position when the
saved viewport fits within the updated bounds.

The pinned KOReader runtime gave identical crop bounds with and
without the authored edge specks. This included marks 1.2 to 1.5
points wide and high. The previous Reader crop expanded from about
19.5% of the page to 48.0% upright and 48.5% with PDF rotation. The
additional authored controls cover shaded paper, faint strokes, a
continuous gutter, sparse notes and the tiny-mark seed fallback. Both
speck fixtures agree with stock within 0.001 point per edge. Shaded
paper and the continuous gutter have one-point rounding differences.

| Authored page | Before | After | KOReader |
| --- | ---: | ---: | ---: |
| Upright with specks | 48.0% | 19.5% | 19.5% |
| Rotated with specks | 48.5% | 19.6% | 19.6% |

Percentages measure the visible crop area relative to the original page.

Set `READER_SCAN_MARGIN_ORACLE_DIR` to a scratch directory and run
`cargo test --lib scan_margin_specks -- --nocapture` to generate the
test documents. Pass those PDFs to `verify-stock-scan-margins.lua`
using the pinned runtime and display settings from the semi-auto crop
comparison. [Stock comparison harness](../scripts/verify-stock-scan-margins.lua) · [Crop setup](semi-auto-pdf-crop.md) · [Simulator before Apply](screenshots/scan-margin-before.png) · [Simulator after Apply](screenshots/scan-margin-after.png)
