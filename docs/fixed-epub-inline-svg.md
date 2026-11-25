# Inline SVG on fixed EPUB pages

Fixed-layout XHTML pages can contain supported static inline SVG illustrations,
including local PNG, JPEG or GIF images referenced from the graphic. Reader
validates each SVG using its existing bounded renderer and supplies a
transparent raster image to the fixed-page layout. The raster is temporary;
the publication's XHTML, reading position and source identity are not changed.
Local images remain confined to the book, and the temporary image provider
never fetches the network.

The page accepts at most 32 inline SVG graphics, a 2-million-pixel budget, and
8 MiB of temporary PNG data. For each graphic, the larger of its output pixels
and total source-image pixels counts toward that budget. Existing document,
resource and paint limits continue to apply. Scripts, remote resources,
unsupported SVG elements and oversized or malformed graphics are rejected.
The generated image preserves the SVG's `id`, `class`, `style`, `width` and
`height`; CSS selectors that specifically target an `svg` element, SVG DOM
interaction and selectable text inside artwork are not reproduced. Separate
SVG spine items retain their existing path.

KOReader registers EPUB with both CREngine and MuPDF. The pinned MuPDF HTML
provider constructs an image from inline SVG; this implementation covers the
bounded subset described above. [KOReader providers](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L378-L382) · [MuPDF inline SVG handler](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/html/html-parse.c#L685-L698) · [Image layout dispatch](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/html/html-parse.c#L1509-L1516)

This change has not been built, visually compared with KOReader or tested in
the simulator or on Kindle. Page sizing, SVG containing a local image,
transparency, CSS placement and failure handling need verification before a
compatibility claim.
