# PDF link destination candidates

Reader resolves local PDF links and Contents entries that identify their target
by an indirect page reference or a zero-based page index. For a `/FitR` link,
Reader transforms the target rectangle through the destination page's crop
and intrinsic rotation and navigates to its upright top-left point. It retains
the current reading zoom; automatic rectangle fitting is not implemented.
Other unsupported or malformed position values retain the existing page-only
fallback. Out-of-range or negative page numbers remain unavailable rather than
being redirected to a different page.

Pinned KOReader uses MuPDF's link resolver. The reference accepts indexed local
pages and parses and transforms `/FitR` rectangles. Reader's source test covers
explicit and named rectangles, rotation, crop, invalid page numbers and an
invalid rectangle. This is an unverified candidate: the tests, KOReader runtime
comparison, simulator and Kindle acceptance have not been run. [MuPDF
reference](https://github.com/ArtifexSoftware/mupdf/blob/master/source/pdf/pdf-link.c) · [Reader implementation and tests](../src/fixed.rs)
