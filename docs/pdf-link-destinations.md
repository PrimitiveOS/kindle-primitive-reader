# PDF link destination candidates

Reader resolves local PDF links and Contents entries that identify their target
by an indirect page reference or a zero-based page index. For a `/FitR` link,
Reader transforms the target rectangle through the destination page's crop
and intrinsic rotation and navigates to its upright top-left point. It retains
the current reading zoom; automatic rectangle fitting is not implemented.
Other unsupported or malformed position values retain the existing page-only
fallback. Out-of-range or negative page numbers remain unavailable rather than
being redirected to a different page.

The local candidate also retains a resolved `/XYZ` point or `/FitR` rectangle
position from a PDF Contents entry. Selecting that entry uses the existing
positioned PDF jump, including the destination page's crop and intrinsic
rotation; a missing or unsupported position still opens the page as before.
This does not implement automatic `/FitR` zoom or positional DjVu and
fixed-layout EPUB outlines. Page labels, saved
bookmarks and source PDF bytes are unchanged. The Contents routing and its
interaction with zoom, rotation, reading history and scrolling remain
behaviorally unverified on the simulator and Kindle.

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
axis and converts through its PDF page transform. Returning through jump
history restores the previous fit, zoom and pan on the same viewport; a changed
viewport uses the saved source point. Page-only fallback remains for null or
malformed coordinates, rotated target pages, a user rotation, crops,
straightening, continuous or spread views, and cases where the render scale cap
prevents the requested exact fit. `/FitBH` and `/FitBV` remain unsupported.
This candidate has not been compared to KOReader or behaviorally verified with
real PDFs, in the simulator or on Kindle.

Pinned KOReader uses MuPDF's link resolver. The reference accepts indexed local
pages and parses and transforms `/FitR` rectangles. Reader's source test covers
explicit and named rectangles, rotation, crop, invalid page numbers and an
invalid rectangle. This is an unverified candidate: the tests, KOReader runtime
comparison, simulator and Kindle acceptance have not been run. [MuPDF
reference](https://github.com/ArtifexSoftware/mupdf/blob/master/source/pdf/pdf-link.c) · [Reader implementation and tests](../src/fixed.rs)
