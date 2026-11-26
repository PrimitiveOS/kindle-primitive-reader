# BMP and portable-anymap illustrations

Reader can decode BMP and portable-anymap (PBM, PGM, PPM, PNM and PAM)
illustrations referenced from supported reflowable and fixed-layout EPUB
chapters. They use the existing bounded image decoder, preserve the original
package path, and participate in the normal image preview and grayscale page
rendering. Existing GIF first-frame and PNG/JPEG handling is unchanged.

The existing side-length, decoded-image allocation and chapter resource
budgets still apply. A file must have a recognized image signature; a renamed
or malformed image does not become a supported image. This change does not
add BMP or portable-anymap resources inside the separate inline-SVG renderer,
and it does not expand the supported image types inside FB2 or HTMLZ packages.

KOReader offers an alternate MuPDF EPUB provider. Its pinned image decoder
handles BMP and PNM image data; the native Reader uses Rust's existing image
decoder instead. [KOReader EPUB provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L378-L382) · [MuPDF BMP and PNM decode](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/fitz/image.c#L805-L819)

This change has not yet been built or tested. Pixel fidelity, malformed-image
limits, transparency and appearance in both reflow and fixed-layout views
need simulator and device verification before a compatibility claim.

Book Information can also attempt a publisher-declared BMP (`image/bmp` or
`image/x-ms-bmp`) or WebP (`image/webp`) cover when no previously supported
cover choice is available. This does not change the book's chapter content or
saved positions. BMP covers use the existing raster signature and decoded-size
checks. WebP covers use the existing bounded simple VP8/VP8L decoder and are
converted to grayscale PNG for inspection, with a 4 MiB input and output
limit. Animated, extended and otherwise unsupported WebP variants are not
accepted. The decoder's internal memory behavior, malformed inputs, real-book
appearance, and exact KOReader cover behavior remain unverified; this candidate
has not yet been behaviorally tested in simulation or on Kindle.

As a separate candidate, EPUB chapter illustrations may now use simple still
VP8 or VP8L WebP. The image display and original-size inspection paths convert
the supported payload to bounded grayscale PNG locally, while the original
archive bytes, package path, XHTML and source anchors remain unchanged. The
existing WebP decoder limits the compressed input to 4 MiB and the decoded
pixel count to 4 million; converted PNGs over 4 MiB are rejected. Extended,
animated, malformed or otherwise unsupported WebP images remain unavailable.
The decoder's internal allocation behavior, exact KOReader comparison, and the
appearance and performance on real EPUBs, in simulation and on Kindle remain
unverified. This does not add WebP references inside the SVG renderer.
