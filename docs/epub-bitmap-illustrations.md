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
