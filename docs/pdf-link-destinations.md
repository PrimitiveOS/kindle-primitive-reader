# PDF link destination candidates

Reader resolves local PDF links and Contents entries that identify their target
by an indirect page reference or a zero-based page index. For supported `/FitR`
links and Contents entries, Reader transforms the destination rectangle through
the target page's crop and intrinsic rotation, fits it to the viewport and
centers it. It records the prior viewing geometry for Back and forward. The
precise scale is stored as an optional bounded fixed-view setting; existing
saved views without it retain their original meaning. Explicit fit or zoom
controls replace it. Because reusable page defaults and reading profiles use
standard fit/zoom presets, choose one of those presets before saving either;
Reader does not silently save the document-specific region scale as page fit.
Other unsupported or malformed position values retain the existing page-only
fallback. Out-of-range or negative page numbers remain unavailable rather than
being redirected to a different page.

The local candidate also retains a resolved `/XYZ` point or `/FitR` rectangle
position from a PDF Contents entry. Selecting that entry uses the existing
positioned PDF jump, including the destination page's crop and intrinsic
rotation; a missing or unsupported position still opens the page as before.
Positional DjVu and fixed-layout EPUB outlines remain unsupported. Page labels, saved
bookmarks and source PDF bytes are unchanged. The Contents routing and its
interaction with zoom, rotation, reading history and scrolling remain
behaviorally unverified on the simulator and Kindle.

The `/FitR` path requires a finite nonempty rectangle fully inside the upright
page, a scale between 0.001 and 64 pixels per PDF point, and a view whose
center is reachable without violating the page-pan or render-size limits.
Crop, user rotation, straightening, spread and continuous views keep the
existing top-left-position fallback. If any check fails, Reader preserves its
current zoom and uses that fallback. Changing viewport size retains the saved
absolute scale and source position rather than recalculating the rectangle fit.
This candidate has not been behaviorally tested against real PDFs, KOReader,
the simulator or Kindle.

For PDF `/XYZ` links and Contents entries with a null left or top coordinate
and a null or zero zoom, the candidate retains the corresponding coordinate
from the current viewport. It converts that position from the current page's
upright crop space to PDF user space, substitutes the specified coordinates,
then converts them through the destination page's crop and intrinsic rotation.
It applies the position only when the current and destination render scales
match exactly. Per-page automatic crops, different effective page scales,
missing viewport geometry, invalid or non-invertible transforms, unsupported
nonzero zoom and malformed coordinates retain the existing page-only fallback.
The fallback can still change effective magnification because Reader uses
fit-relative zoom. Complete coordinate destinations keep their earlier
behavior. This path has not been compared to KOReader or behaviorally verified
with real PDFs, in the simulator or on Kindle.

Local `/FitH top` and `/FitV left` links and Contents entries switch to width
fit or height fit, respectively, and position the specified finite coordinate.
The candidate uses the target page's unrotated crop edge for the unspecified
axis and converts through its PDF page transform. A null `top` or `left`
retains that coordinate from the current viewport after conversion through
both PDF page transforms; if the current coordinate cannot be proved, the
existing page-only fallback remains. [PDF 1.7 destination semantics](https://opensource.adobe.com/dc-acrobat-sdk-docs/standards/pdfstandards/pdf/PDF32000_2008.pdf)
Returning through jump
history restores the previous fit, zoom and pan on the same viewport; a changed
viewport uses the saved source point. Page-only fallback remains for missing or
malformed coordinates, rotated target pages, a user rotation, crops,
straightening, continuous or spread views, and cases where the render scale cap
prevents the requested exact fit. `/FitBH` and `/FitBV` remain unsupported.
This candidate has not been compared to KOReader or behaviorally verified with
real PDFs, in the simulator or on Kindle.

Local `/Fit` links and Contents entries now switch to whole-page fit and center
the target page. This works with the PDF page's intrinsic rotation and preserves
the prior fit, zoom and pan in jump history when the fit changes. The same
conservative crop, user-rotation, continuous/spread and render-scale restrictions
apply; otherwise Reader keeps the page-only fallback. `/FitB` uses a content
bounding box and remains unsupported. MuPDF distinguishes `/Fit` and `/FitB`
as separate destination types in its [PDF link resolver](https://github.com/ArtifexSoftware/mupdf/blob/master/source/pdf/pdf-link.c).
This candidate and its history behavior have not been behaviorally tested.

Pinned KOReader uses MuPDF's link resolver. The reference accepts indexed local
pages and parses and transforms `/FitR` rectangles. Reader's source test covers
explicit and named rectangles, rotation, crop, invalid page numbers and an
invalid rectangle. This is an unverified candidate: the tests, KOReader runtime
comparison, simulator and Kindle acceptance have not been run. [MuPDF
reference](https://github.com/ArtifexSoftware/mupdf/blob/master/source/pdf/pdf-link.c) · [Reader implementation and tests](../src/fixed.rs)
