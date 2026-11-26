# BMP and portable-anymap illustrations

Reader can decode BMP and portable-anymap (PBM, PGM, PPM, PNM and PAM)
illustrations referenced from supported reflowable and fixed-layout EPUB
chapters. They use the existing bounded image decoder, preserve the original
package path, and participate in the normal image preview and grayscale page
rendering. Existing GIF first-frame and PNG/JPEG handling is unchanged.

The existing side-length, decoded-image allocation and chapter resource
budgets still apply. A file must have a recognized image signature; a renamed
or malformed image does not become a supported image. This change does not
expand the supported image types inside FB2 or HTMLZ packages.

KOReader offers an alternate MuPDF EPUB provider. Its pinned image decoder
handles BMP and PNM image data; the native Reader uses Rust's existing image
decoder instead. [KOReader EPUB provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L378-L382) · [MuPDF BMP and PNM decode](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/fitz/image.c#L805-L819)

This change is included in the current Mac build-only checkpoint; it has not
been behaviorally tested. Pixel fidelity, malformed-image limits, transparency
and appearance in both reflow and fixed-layout views need simulator and device
verification before a compatibility claim.

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

The same fallback can attempt a publisher-declared TIFF (`image/tiff`) cover.
It converts the existing supported single-page TIFF subset to grayscale PNG in
memory for Book Information. Input and converted output are each capped at
4 MiB; the TIFF decoder's dimension, strip and pixel limits still apply. Other
TIFF variants remain unsupported, and decoder/encoder peak memory, appearance,
KOReader comparison, real-document, simulator and Kindle behavior are not yet
verified. The publication bytes, chapter content and source positions do not
change.

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

Reader can also attempt TIFF illustrations in EPUB chapters and their
original-size inspection view. It converts a recognized TIFF payload with the
existing bounded TIFF decoder to grayscale PNG only at the image-consumer
boundary; package bytes, resource paths, XHTML and source anchors remain
unchanged. Input and converted output are each capped at 8 MiB, and the TIFF
decoder's existing dimension, pixel and strip limits apply. Only that decoder's
single-page subset is supported; multi-page TIFF, unsupported compression,
color-management and other unsupported variants remain unavailable. PNG encoder
and decoder transient memory, image fidelity, real-book appearance, simulation
and Kindle behavior remain unverified. KOReader registers EPUB with its MuPDF
provider; the pinned MuPDF image reader routes TIFF to its own decoder, whose
full behavior has not been compared. [MuPDF TIFF path](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/fitz/image.c#L814-L815)

The inline and package SVG renderer can also attempt local BMP and portable
anymap image resources. Existing PNG, JPEG and GIF handling is unchanged.
The additional formats use recognized signatures, the existing dimension and
source-pixel budget, a 24 MiB declared decoder-output limit, and an alpha-
preserving RGBA PNG conversion. Original and converted bytes both count toward
the 8 MiB aggregate SVG raster budget; a capped writer stops PNG output at the
remaining budget. A conservative 32 MiB accounting limit includes source bytes,
declared decoder output, RGBA pixels and twice the remaining output allowance
for vector growth. Decoder/encoder internal scratch and allocator overhead are
not proven by this accounting. External URLs remain unavailable, and source paths, XHTML and anchors remain
unchanged. Exact KOReader behavior,
malformed inputs, alpha fidelity, peak memory and real-book, simulator and
Kindle behavior are unverified.

Book-aware inline and package SVG rendering can also attempt self-contained
canonical `data:image/png;base64,...`, `data:image/jpeg;base64,...` and
`data:image/gif;base64,...` image references. Declared MIME types must match the
image signature; noncanonical encoding, other MIME types and external URLs are
rejected. The containing SVG stays within 256 KiB, and embedded images use the
same eight-image, source-pixel, decoder and aggregate 8 MiB raster budgets as
local references, including decoded GIF conversion costs. The SVG parser may
create a second bounded copy of the decoded data. Standalone SVG and SVG-only comic pages can use this same embedded-image
subset as a fallback when the vector-only path fails. Standalone resources
must be embedded data: relative paths, file URLs and network URLs are rejected.
The image data is not extracted or fetched, and source XML and anchors remain
unchanged. KOReader parity, malformed-input and
memory behavior, and real-book, simulator and Kindle validation remain open.

Reflow HTML/XHTML `<img>` also accepts canonical base64 PNG, JPEG and GIF data
images through the same strict MIME/signature check. The encoded payload is
limited to 256 KiB; the usual source-pixel, illustration-cache and first-frame
GIF limits still apply. A SHA-256 identity is stored for each rendered image
instead of copying the potentially large URI into every repeated image record.
Opening the original image rechecks the chapter source digest and decodes the
matching source URI again. Publisher XHTML, text and saved anchors remain
unchanged. Noncanonical encodings, animated GIF playback and network images
remain outside this subset. Peak decoder memory and real-book,
simulator and Kindle behavior remain unverified. Pinned KOReader recognizes
data-image references in its [document engine](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L16164-L16165).

Reflow HTML/XHTML image elements can also attempt self-contained SVG data URIs
using exact `data:image/svg+xml,` percent-encoded UTF-8 or canonical
`data:image/svg+xml;base64,` encoding. Both encoded and decoded payloads are
limited to 256 KiB and pass through the existing bounded standalone static-SVG
validator and rasterizer. Malformed percent escapes, URI fragments, other MIME
parameters, external or relative dependencies and unsupported SVG features are
rejected. Supported embedded PNG/JPEG/GIF data images remain subject to the
standalone SVG image budgets. The source XHTML, resource identity and saved
anchors are unchanged; opening an illustration again verifies its source digest.
KOReader parity, malformed-input behavior, renderer peak memory, real-book,
simulator and Kindle verification remain open.

Reflow JPEG illustrations with a supported non-identity EXIF orientation use
that orientation for both the page image and its original-image viewer. The
oriented dimensions determine the CSS image fit, so affected books can
repaginate; package bytes, resource paths and saved text anchors are unchanged.
JPEGs with no readable orientation or an identity orientation retain the prior
decoding path. The existing 4096-pixel-side, 8-million-pixel and 24 MiB decoded
image limits apply. Orientation is applied after grayscale conversion to bound
the extra rotated buffer, but peak decoder memory and malformed-metadata
behavior remain unverified. KOReader comparison and real-book, simulator and
Kindle verification are also open.

The static SVG candidate accepts a bounded inline `style` attribute on SVG
elements for supported paint, stroke, fill/stroke/stop opacity, visibility and rendering-hint
properties. Simple colors, numeric values, and local `url(#gradient)` paint
references are accepted; CSS escapes, quoted strings, comments, external URLs,
effects, arbitrary functions and unsupported declarations are rejected.
Each style is limited to 4 KiB, 64 declarations and 256 bytes per value; the
existing SVG document, pixel, resource and paint-work limits still apply.
This covers inline styles only, not SVG stylesheet elements. Existing accepted
SVG paths, package bytes and reading anchors are unchanged. KOReader behavior,
malformed inputs, memory use and real-book, simulator and Kindle rendering
remain unverified.

Supported static SVG can also use up to 128 local `<use href="#id">` or
`xlink:href` references to uniquely identified primitive leaf shapes or groups
containing only nested static groups and primitive leaves. The sum of
referenced source bytes is capped at 512 KiB and expanded elements at 4,096
before SVG expansion; the existing document, pixel and paint-work limits still
apply. External or ambiguous targets, chained uses, images in groups, symbols
and referenced primitive elements with children are excluded. Package data
and reading anchors remain unchanged; KOReader comparison, rendering fidelity,
peak memory and real-book, simulator and Kindle behavior remain unverified.

Standalone SVG embedded-image fallback uses the same 256 KiB input, eight
unique images, pixel and 8 MiB aggregate raster limits, plus the existing GIF
conversion checks. Previously successful vector-only output is tried first.
Reader may hold decoded payload and parser copies concurrently; GIF conversion,
decoder internals and allocator overhead still need peak-memory validation.
KOReader registers SVG with its MuPDF provider, but its exact embedded-image
behavior has not been compared. [Pinned SVG provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L413)
